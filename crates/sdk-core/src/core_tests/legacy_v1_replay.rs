use crate::{
    internal_flags::CoreInternalFlags,
    prost_dur,
    replay::{HistoryForReplay, HistoryInfo, ReplayWorkerInput},
    test_help::{drain_pollers_and_shutdown, test_worker_cfg},
    worker::client::{WorkerClient, WorkflowTaskCompletion, mocks::mock_manual_worker_client},
};
use futures_util::{FutureExt, StreamExt, stream};
use prost::Message;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use temporalio_common::protos::{
    coresdk::{
        activity_result::activity_resolution,
        workflow_activation::{remove_from_cache::EvictionReason, workflow_activation_job},
        workflow_commands::{ScheduleActivity, StartTimer, workflow_command::Variant},
        workflow_completion::WorkflowActivationCompletion,
    },
    temporal::api::{
        enums::v1::{EventType, UpdateAdmittedEventOrigin, WorkflowTaskFailedCause},
        failure::v1::Failure,
        history::v1::{History, history_event::Attributes},
    },
};

type Terminal = Result<WorkflowTaskCompletion, (WorkflowTaskFailedCause, Option<Failure>)>;

#[tokio::test]
async fn flagless_admitted_history_preserves_legacy_replay() {
    let bytes = include_bytes!("../../tests/histories/legacy_v1_admitted_only.bin");
    assert_eq!(bytes.len(), 3868);
    let history = History::decode(bytes.as_slice()).unwrap();
    assert_eq!(history.events.len(), 22);
    for (index, event) in history.events.iter().enumerate() {
        assert_eq!(event.event_id, index as i64 + 1);
        assert!(!matches!(
            event.event_type(),
            EventType::WorkflowExecutionUpdateAccepted
                | EventType::WorkflowExecutionUpdateCompleted
                | EventType::WorkflowTaskFailed
                | EventType::WorkflowTaskTimedOut
        ));
        if let Some(Attributes::WorkflowTaskCompletedEventAttributes(attrs)) = &event.attributes {
            assert!(attrs.sdk_metadata.is_none());
        }
    }
    assert_eq!(
        history.events[15..]
            .iter()
            .map(|event| event.event_type())
            .collect::<Vec<_>>(),
        [
            EventType::WorkflowTaskCompleted,
            EventType::WorkflowTaskScheduled,
            EventType::WorkflowExecutionUpdateAdmitted,
            EventType::WorkflowExecutionSignaled,
            EventType::WorkflowTaskStarted,
            EventType::WorkflowTaskCompleted,
            EventType::TimerStarted
        ]
    );
    let Some(Attributes::WorkflowExecutionStartedEventAttributes(start)) =
        &history.events[0].attributes
    else {
        panic!("missing workflow start")
    };
    let workflow_id = start.workflow_id.clone();
    let run_id = start.original_execution_run_id.clone();
    let task_queue = start.task_queue.as_ref().unwrap().name.clone();
    let Some(Attributes::WorkflowExecutionUpdateAdmittedEventAttributes(admitted)) =
        &history.events[17].attributes
    else {
        panic!("missing Admitted")
    };
    assert_eq!(admitted.origin, UpdateAdmittedEventOrigin::Reapply as i32);
    let Some(Attributes::WorkflowExecutionSignaledEventAttributes(signal)) =
        &history.events[18].attributes
    else {
        panic!("missing Signal")
    };
    let signal = signal.clone();
    assert_eq!(signal.signal_name, "snapshot-item-1");
    assert_eq!(
        signal.input.as_ref().unwrap().payloads[0].data.as_slice(),
        br#""choose-timer""#
    );
    let Some(Attributes::TimerStartedEventAttributes(timer)) = &history.events[21].attributes
    else {
        panic!("missing TimerStarted")
    };
    assert_eq!(timer.timer_id, "3");
    assert_eq!(timer.workflow_task_completed_event_id, 21);
    assert_eq!(
        timer.start_to_fire_timeout,
        Some(prost_dur!(from_secs(3600)))
    );

    // Match the official replay envelope, which treats the last recorded WFT as historical.
    let info = HistoryInfo::new_from_history(&history, None).unwrap();
    let envelope = info.as_poll_wft_response();
    assert_eq!(envelope.history.as_ref(), Some(&history));
    assert_eq!(
        (
            envelope.previous_started_event_id,
            envelope.started_event_id
        ),
        (20, 20)
    );
    assert!(envelope.messages.is_empty());
    assert!(envelope.next_page_token.is_empty());
    assert_eq!(info.orig_run_id(), run_id);

    let terminals: Arc<Mutex<Vec<Terminal>>> = Arc::new(Mutex::new(Vec::new()));
    let mut client = mock_manual_worker_client();
    assert!(client.capabilities().unwrap().sdk_metadata);
    let successes = terminals.clone();
    client
        .expect_complete_workflow_task()
        .returning(move |completion, _| {
            eprintln!(
                "replay completion flags={:?}",
                completion.sdk_metadata.core_used_flags
            );
            successes.lock().unwrap().push(Ok(completion));
            async { Ok(Default::default()) }.boxed()
        });
    let failures = terminals.clone();
    client
        .expect_fail_workflow_task()
        .returning(move |_, cause, failure| {
            eprintln!("SERVER_HISTORY_CORE_FAILURE cause={cause:?} failure={failure:#?}");
            failures.lock().unwrap().push(Err((cause, failure)));
            async { Ok(Default::default()) }.boxed()
        });
    let mut config = test_worker_cfg().build().unwrap();
    config.task_queue = task_queue.clone();
    // Keep the stream open until the replay-generated eviction exposes the last processed ID.
    let histories = stream::iter([HistoryForReplay::new(history, workflow_id.clone())])
        .chain(stream::pending());
    let mut replay = ReplayWorkerInput::new(config, histories);
    replay.client_override = Some(client);
    let core = replay.into_core_worker().unwrap();
    core.validate().await.unwrap();
    let mut boundaries = Vec::new();
    let mut job_orders = Vec::new();
    let mut dms: [Option<String>; 2] = Default::default();
    let mut snapshot = None;
    let mut signal_command = "";
    let mut issued_timer = None;
    let eviction = tokio::time::timeout(Duration::from_secs(30), async {
        let eviction = loop {
            let activation = core.poll_workflow_activation().await.unwrap();
            assert_eq!(activation.run_id, run_id);
            if activation.is_only_eviction() {
                let workflow_activation_job::Variant::RemoveFromCache(reason) =
                    activation.jobs[0].variant.as_ref().unwrap()
                else {
                    unreachable!()
                };
                let eviction = (
                    activation.history_length,
                    reason.reason(),
                    reason.message.clone(),
                );
                core.complete_workflow_activation(WorkflowActivationCompletion::empty(
                    activation.run_id,
                ))
                .await
                .unwrap();
                break eviction;
            }
            boundaries.push(activation.history_length);
            let mut jobs = Vec::new();
            let mut commands: Vec<Variant> = Vec::new();
            for job in activation.jobs {
                match job.variant.unwrap() {
                    workflow_activation_job::Variant::InitializeWorkflow(init) => {
                        jobs.push("initialize".to_string());
                        assert_eq!(init.workflow_type, "func1");
                        assert_eq!(init.workflow_id, workflow_id);
                        for item in 0..2 {
                            // The Server fills its default retry policy for these omitted policies.
                            commands.push(
                                ScheduleActivity {
                                    seq: item + 1,
                                    activity_id: format!("item-{item}"),
                                    activity_type: "item".to_string(),
                                    task_queue: task_queue.clone(),
                                    start_to_close_timeout: Some(prost_dur!(from_secs(300))),
                                    ..Default::default()
                                }
                                .into(),
                            );
                        }
                    }
                    workflow_activation_job::Variant::ResolveActivity(activity) => {
                        jobs.push(format!("resolve:{}", activity.seq));
                        assert!((1..=2).contains(&activity.seq));
                        let Some(activity_resolution::Status::Completed(result)) =
                            activity.result.unwrap().status
                        else {
                            panic!("expected successful activity")
                        };
                        let payload = result.result.unwrap();
                        assert_eq!(
                            payload.metadata.get("encoding").unwrap().as_slice(),
                            b"json/plain"
                        );
                        let slot = &mut dms[(activity.seq - 1) as usize];
                        assert!(slot.is_none());
                        *slot = Some(serde_json::from_slice(&payload.data).unwrap());
                    }
                    workflow_activation_job::Variant::SignalWorkflow(observed) => {
                        jobs.push(format!("signal:{}", observed.signal_name));
                        assert_eq!(observed.signal_name, signal.signal_name);
                        assert_eq!(observed.input, signal.input.as_ref().unwrap().payloads);
                        assert_eq!(observed.identity, signal.identity);
                        snapshot = dms[1].clone();
                        // Retain both branches so a boundary change reaches command matching.
                        if snapshot.is_none() {
                            signal_command = "StartTimer";
                            let command = StartTimer {
                                seq: 3,
                                start_to_fire_timeout: Some(prost_dur!(from_secs(3600))),
                            };
                            issued_timer = Some(command.clone());
                            commands.push(command.into());
                        } else {
                            signal_command = "ScheduleActivity";
                            commands.push(
                                ScheduleActivity {
                                    seq: 3,
                                    activity_id: "state-was-ready".to_string(),
                                    activity_type: "item".to_string(),
                                    task_queue: task_queue.clone(),
                                    start_to_close_timeout: Some(prost_dur!(from_secs(300))),
                                    ..Default::default()
                                }
                                .into(),
                            );
                        }
                        eprintln!(
                            "SERVER_HISTORY_SIGNAL snapshot={snapshot:?} command={signal_command}"
                        );
                    }
                    workflow_activation_job::Variant::DoUpdate(update) => {
                        panic!("historical admitted-only U must not execute again: {update:?}");
                    }
                    other => panic!("unexpected job in 22-event replay: {other:?}"),
                }
            }
            eprintln!(
                "legacy replay boundary={} jobs={jobs:?} commands={commands:?}",
                activation.history_length
            );
            job_orders.push(jobs);
            core.complete_workflow_activation(WorkflowActivationCompletion::from_cmds(
                activation.run_id,
                commands,
            ))
            .await
            .unwrap();
        };
        drain_pollers_and_shutdown(&core).await;
        eviction
    })
    .await
    .expect("Server history replay did not finish and expose its processed event ID");
    let terminals = std::mem::take(&mut *terminals.lock().unwrap());
    let matched = matches!(terminals.as_slice(), [Ok(completion)] if completion.commands.is_empty() && completion.messages.is_empty())
        && eviction.0 == 22
        && eviction.1 == EvictionReason::LangRequested;
    eprintln!(
        "LEGACY_REPLAY writer={:?} boundaries={boundaries:?} jobs={job_orders:?} signal_state={snapshot:?} command={signal_command} event_22_matched={matched}",
        std::env::var("TEMPORAL_USE_WFT_CHUNKING_V2")
    );
    assert_eq!(dms, [Some("dm-0".to_string()), Some("dm-1".to_string())]);
    assert_eq!(
        terminals.len(),
        1,
        "expected exactly one terminal replay RPC: {terminals:#?}"
    );
    let completion = terminals
        .into_iter()
        .next()
        .unwrap()
        .expect("real Server History must replay without a command mismatch");
    assert!(
        completion.commands.is_empty(),
        "historical commands were not consumed: {:?}",
        completion.commands
    );
    assert!(completion.messages.is_empty());
    assert!(completion.query_responses.is_empty());
    assert!(
        !completion
            .sdk_metadata
            .core_used_flags
            .contains(&(CoreInternalFlags::WftChunkingV2 as u32))
    );
    assert_eq!(
        eviction.0, 22,
        "replay must process TimerStarted, not stop after Signal"
    );
    assert_eq!(eviction.1, EvictionReason::LangRequested);
    assert!(matched);
    assert_eq!(snapshot, None);
    assert_eq!(signal_command, "StartTimer");
    let timer = issued_timer.unwrap();
    assert_eq!(timer.seq, 3);
    assert_eq!(
        timer.start_to_fire_timeout,
        Some(prost_dur!(from_secs(3600)))
    );
    // V2 would retain boundary 15 and change the Signal's command. This checks production
    // version selection through the replay worker, including when its V2 writer is enabled.
    assert_eq!(boundaries, [3, 10, 20]);
    assert_eq!(
        job_orders,
        [
            vec!["initialize"],
            vec!["resolve:1"],
            vec!["signal:snapshot-item-1", "resolve:2"]
        ]
    );
}
