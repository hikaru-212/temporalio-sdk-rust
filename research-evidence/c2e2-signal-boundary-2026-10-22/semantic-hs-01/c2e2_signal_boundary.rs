use crate::worker::workflow::chunking::{RunVersion, boundary, command_event};
use prost::Message;
use std::{fs, io::Write, path::Path};
use temporalio_common::protos::{
    coresdk::common::extract_local_activity_marker_details,
    temporal::api::{
        common::v1::Payloads,
        enums::v1::EventType,
        history::v1::{
            History, HistoryEvent, WorkflowExecutionSignaledEventAttributes,
            history_event::Attributes,
        },
    },
};

const T: i64 = 1_800_000_000;

fn save(path: &Path, bytes: &[u8]) {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    file.write_all(bytes).unwrap();
    file.sync_all().unwrap();
}

fn fixture_pair() -> (History, History) {
    let h0_bytes = fs::read(std::env::var("C2E2_INPUT").unwrap()).unwrap();
    let h0 = History::decode(h0_bytes.as_slice()).unwrap();
    validate_fixture(&h0, false);
    let mut hs = h0.clone();
    for event in hs.events.iter_mut().skip(5) {
        event.event_id += 1;
        match event.attributes.as_mut().unwrap() {
            Attributes::WorkflowTaskStartedEventAttributes(attrs) => {
                if attrs.scheduled_event_id >= 6 {
                    attrs.scheduled_event_id += 1;
                }
            }
            Attributes::WorkflowTaskCompletedEventAttributes(attrs) => {
                if attrs.scheduled_event_id >= 6 {
                    attrs.scheduled_event_id += 1;
                }
                if attrs.started_event_id >= 6 {
                    attrs.started_event_id += 1;
                }
            }
            Attributes::MarkerRecordedEventAttributes(attrs) => {
                attrs.workflow_task_completed_event_id += 1;
            }
            Attributes::WorkflowExecutionCompletedEventAttributes(attrs) => {
                attrs.workflow_task_completed_event_id += 1;
            }
            Attributes::WorkflowTaskScheduledEventAttributes(_) => {}
            other => panic!("unexpected event in frozen C2-D tail: {other:?}"),
        }
    }
    hs.events.insert(
        5,
        HistoryEvent {
            event_id: 6,
            event_time: h0.events[4].event_time,
            event_type: EventType::WorkflowExecutionSignaled as i32,
            attributes: Some(Attributes::WorkflowExecutionSignaledEventAttributes(
                WorkflowExecutionSignaledEventAttributes {
                    signal_name: "probe".to_owned(),
                    input: Some(Payloads { payloads: vec![] }),
                    identity: "c2-client".to_owned(),
                    request_id: "00000000-0000-4000-8000-000000000006".to_owned(),
                    ..Default::default()
                },
            )),
            ..Default::default()
        },
    );
    assert_eq!(h0.events[..5], hs.events[..5]);
    for (old, new) in h0.events.iter().skip(5).zip(hs.events.iter().skip(6)) {
        assert_eq!(old.event_time, new.event_time);
        assert_eq!(old.event_type, new.event_type);
    }
    validate_fixture(&hs, true);
    (h0, hs)
}

fn validate_fixture(history: &History, has_signal: bool) {
    let shift = i64::from(has_signal);
    let middle_started = 6 + shift;
    let final_scheduled = 8 + shift;
    let final_started = 9 + shift;
    let final_completed = 10 + shift;
    let marker_id = 11 + shift;
    let terminal_id = 12 + shift;
    assert_eq!(history.events.len(), terminal_id as usize);
    let expected = if has_signal {
        vec![
            EventType::WorkflowExecutionStarted,
            EventType::WorkflowTaskScheduled,
            EventType::WorkflowTaskStarted,
            EventType::WorkflowTaskCompleted,
            EventType::WorkflowTaskScheduled,
            EventType::WorkflowExecutionSignaled,
            EventType::WorkflowTaskStarted,
            EventType::WorkflowTaskCompleted,
            EventType::WorkflowTaskScheduled,
            EventType::WorkflowTaskStarted,
            EventType::WorkflowTaskCompleted,
            EventType::MarkerRecorded,
            EventType::WorkflowExecutionCompleted,
        ]
    } else {
        vec![
            EventType::WorkflowExecutionStarted,
            EventType::WorkflowTaskScheduled,
            EventType::WorkflowTaskStarted,
            EventType::WorkflowTaskCompleted,
            EventType::WorkflowTaskScheduled,
            EventType::WorkflowTaskStarted,
            EventType::WorkflowTaskCompleted,
            EventType::WorkflowTaskScheduled,
            EventType::WorkflowTaskStarted,
            EventType::WorkflowTaskCompleted,
            EventType::MarkerRecorded,
            EventType::WorkflowExecutionCompleted,
        ]
    };
    let event_at = |id: i64| &history.events[id as usize - 1];
    for (index, (event, expected_type)) in history.events.iter().zip(expected).enumerate() {
        assert_eq!(event.event_id, index as i64 + 1);
        assert_eq!(event.event_type(), expected_type);
        assert_eq!(command_event(event), event.event_id >= marker_id);
        let timestamp = event.event_time.as_ref().unwrap();
        assert_eq!(timestamp.nanos, 0);
        let expected_time = if event.event_id == 1 {
            T - 1
        } else if event.event_id <= 3 {
            T
        } else if event.event_id <= middle_started {
            T + 8
        } else if event.event_id <= final_started {
            T + 16
        } else {
            T + 17
        };
        assert_eq!(timestamp.seconds, expected_time);
        assert!(!event.worker_may_ignore);
    }
    for (scheduled, started, completed) in [
        (2, 3, 4),
        (5, middle_started, middle_started + 1),
        (final_scheduled, final_started, final_completed),
    ] {
        let Some(Attributes::WorkflowTaskStartedEventAttributes(attrs)) =
            &event_at(started).attributes
        else {
            panic!("expected WorkflowTaskStarted");
        };
        assert_eq!(attrs.scheduled_event_id, scheduled);
        let Some(Attributes::WorkflowTaskCompletedEventAttributes(attrs)) =
            &event_at(completed).attributes
        else {
            panic!("expected WorkflowTaskCompleted");
        };
        assert_eq!(attrs.scheduled_event_id, scheduled);
        assert_eq!(attrs.started_event_id, started);
    }
    let Some(Attributes::MarkerRecordedEventAttributes(marker)) = &event_at(marker_id).attributes
    else {
        panic!("expected Local Activity marker");
    };
    assert_eq!(marker.marker_name, "core_local_activity");
    assert_eq!(marker.workflow_task_completed_event_id, final_completed);
    assert!(marker.failure.is_none());
    let (marker_data, result) = extract_local_activity_marker_details(&mut marker.details.clone());
    let marker_data = marker_data.expect("frozen marker must decode");
    assert_eq!(marker_data.seq, 1);
    assert_eq!(marker_data.attempt, 1);
    assert_eq!(marker_data.activity_id, "la-1");
    assert_eq!(marker_data.activity_type, "c2_la");
    assert_eq!(marker_data.complete_time.unwrap().seconds, T + 17);
    assert_eq!(marker_data.activation_index, Some(0));
    assert!(marker_data.original_schedule_time.is_some());
    assert!(marker_data.backoff.is_none());
    assert_eq!(result.unwrap().data, b"local-result");
    let Some(Attributes::WorkflowExecutionCompletedEventAttributes(terminal)) =
        &event_at(terminal_id).attributes
    else {
        panic!("expected WorkflowExecutionCompleted");
    };
    assert_eq!(terminal.workflow_task_completed_event_id, final_completed);
    assert_eq!(terminal.result.as_ref().unwrap().payloads.len(), 1);
    assert_eq!(terminal.result.as_ref().unwrap().payloads[0].data, b"workflow-result");
    let mut version = RunVersion::default();
    version.observe(&history.events[..4]).unwrap();
    assert_eq!(version.selected(), Some(true));
    if has_signal {
        let signal = event_at(6);
        assert!(!command_event(signal));
        let Some(Attributes::WorkflowExecutionSignaledEventAttributes(attrs)) = &signal.attributes
        else {
            panic!("expected probe Signal");
        };
        assert_eq!(attrs.signal_name, "probe");
        assert!(attrs.input.as_ref().unwrap().payloads.is_empty());
        assert!(attrs.external_workflow_execution.is_none());
    }
}

#[test]
fn scanner_preserves_signal_boundary() {
    let out = std::path::PathBuf::from(std::env::var("C2E2_OUTPUT_DIR").unwrap());
    let (h0, hs) = fixture_pair();
    save(&out.join("H0.pb"), &fs::read(std::env::var("C2E2_INPUT").unwrap()).unwrap());
    save(&out.join("Hs.pb"), &hs.encode_to_vec());
    save(&out.join("H0.decoded.txt"), format!("{h0:#?}\n").as_bytes());
    save(&out.join("Hs.decoded.txt"), format!("{hs:#?}\n").as_bytes());
    let mut observations = Vec::new();
    for (label, history, after, expected_index, expected_event) in [
        ("H0", &h0, 3, 8, 9),
        ("Hs", &hs, 3, 6, 7),
        ("Hs", &hs, 7, 9, 10),
    ] {
        let actual = boundary(&history.events, after, true, &[]);
        let event = actual.map(|index| history.events[index].event_id);
        let observation = format!(
            "{label}: after={after}, finished=true, updates=[]; expected_index={expected_index}, expected_event={expected_event}; actual_index={actual:?}, actual_event={event:?}"
        );
        println!("C2E2 SCANNER {observation}");
        observations.push(observation);
        assert_eq!(actual, Some(expected_index));
        assert_eq!(event, Some(expected_event));
    }
    save(&out.join("scanner-results.txt"), observations.join("\n").as_bytes());
}


#[path = "c2e2_guest.rs"]
mod guest;

use self::guest::C2e2LanguageDriver;
use super::WorkflowManager;
use crate::{
    telemetry::metrics::MetricsContext,
    test_help::test_worker_cfg,
    worker::{
        LocalActRequest, LocalActivityExecutionResult, LocalActivityResolution,
        workflow::{LocalResolution, RunBasics, WFCommand, history_update::HistoryUpdate},
    },
};
use std::time::{Duration, UNIX_EPOCH};
use temporalio_common::protos::{
    coresdk::{
        activity_result::Success,
        workflow_activation::{WorkflowActivation, workflow_activation_job::Variant},
        workflow_commands::WorkflowCommand,
    },
    temporal::api::{
        command::v1::{Command, command::Attributes as CommandAttributes},
        workflowservice::v1::get_system_info_response::Capabilities,
    },
};

const RUN_ID: &str = "00000000-0000-4000-8000-000000000001";

fn manager() -> WorkflowManager {
    WorkflowManager::new(RunBasics {
        worker_config: {
            let mut config = test_worker_cfg().build().unwrap();
            config.task_queue = "c2_queue".to_owned();
            config.into()
        },
        workflow_id: "c2-workflow-id".to_owned(),
        workflow_type: "c2_wf".to_owned(),
        run_id: RUN_ID.to_owned(),
        history: HistoryUpdate::dummy(),
        metrics: MetricsContext::no_op(),
        capabilities: &Capabilities { sdk_metadata: true, ..Default::default() },
        sdk_name: "c2d-guest-adapter", sdk_version: "pinned",
    })
}

fn update(events: &[HistoryEvent], previous: i64, started: i64) -> HistoryUpdate {
    let (update, remaining) = HistoryUpdate::from_events(events.to_vec(), previous, started, true);
    assert!(remaining.is_empty());
    assert_eq!(update.get_events(), events);
    println!("C2E2 HANDOFF previous={previous} started={started} finished=true ids={:?}",
        events.iter().map(|e| e.event_id).collect::<Vec<_>>());
    update
}

fn activation(label: &str, manager: &WorkflowManager, act: &WorkflowActivation, out: &Path) {
    println!("C2E2 ACT {label} machines_replaying={} cursor={} last_processed={} full={act:?}",
        manager.machines.replaying, manager.machines.get_last_wft_started_id(),
        manager.machines.last_processed_event);
    save(&out.join(format!("{label}.activation.pb")), &act.encode_to_vec());
}

fn respond(label: &str, manager: &mut WorkflowManager, lang: &mut C2e2LanguageDriver,
    act: WorkflowActivation) -> Vec<WorkflowCommand> {
    let commands = lang.activate(act);
    println!("C2E2 LANG_COMMANDS {label} {commands:?}");
    manager.push_commands_and_iterate(commands.clone().into_iter()
        .map(|c| WFCommand::try_from(c).unwrap()).collect()).unwrap();
    commands
}

fn completion(manager: &mut WorkflowManager, event: &mut HistoryEvent) -> Vec<Command> {
    let commands = manager.machines.get_commands().collect::<Vec<_>>();
    let metadata = manager.machines.get_metadata_for_wft_complete();
    println!("C2E2 LIVE_WFT_COMPLETION id={} commands={commands:?} metadata={metadata:?}", event.event_id);
    let Some(Attributes::WorkflowTaskCompletedEventAttributes(attrs)) = &mut event.attributes else {
        panic!("expected completed WFT");
    };
    attrs.sdk_metadata = Some(metadata);
    commands
}

#[test]
fn warm_cold_signal_compatibility() {
    let subscriber = tracing_subscriber::fmt().with_max_level(tracing::Level::TRACE)
        .without_time().with_ansi(false)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::NEW).finish();
    let _subscriber = tracing::subscriber::set_default(subscriber);
    assert!(crate::worker::workflow::chunking_v2_enabled());
    let family = std::env::var("C2E2_FAMILY").unwrap();
    assert!(family == "H0" || family == "Hs");
    let has_signal = family == "Hs";
    let out = std::path::PathBuf::from(std::env::var("C2E2_OUTPUT_DIR").unwrap());
    let (h0, hs) = fixture_pair();
    let original = if has_signal { hs } else { h0 };
    let mut history = original.clone();
    let middle = if has_signal { 7usize } else { 6 };
    let last = if has_signal { 10usize } else { 9 };
    let marker_index = last + 1;
    let terminal_index = last + 2;
    let Some(Attributes::MarkerRecordedEventAttributes(marker)) = &history.events[marker_index].attributes else {
        panic!("validated LA marker missing");
    };
    let (original_marker, result) = extract_local_activity_marker_details(&mut marker.details.clone());
    let original_marker = original_marker.unwrap();
    let result = result.unwrap();
    let Some(Attributes::WorkflowExecutionCompletedEventAttributes(terminal)) = &history.events[terminal_index].attributes else {
        panic!("validated terminal missing");
    };
    let workflow_result = terminal.result.as_ref().unwrap().payloads[0].clone();
    save(&out.join("input.decoded.txt"), format!("{original:#?}\n").as_bytes());
    println!("C2E2 FAMILY {family} PATH warm");
    let mut live = manager();
    let first = live.new_work_from_server(update(&history.events[..3], 0, 3), vec![]).unwrap();
    activation("warm-init", &live, &first, &out);
    let mut warm_lang = C2e2LanguageDriver::new(&first, "c2-namespace", "c2_queue",
        result.clone(), workflow_result.clone(), None);
    let warm_initial_commands = respond("warm-init", &mut live, &mut warm_lang, first);
    let requests = live.drain_queued_local_activities();
    let [LocalActRequest::New(request)] = requests.as_slice() else {
        panic!("expected exactly one LA request");
    };
    assert_eq!(request.schedule_cmd.seq, 1);
    assert_eq!(request.schedule_cmd.activity_id, "la-1");
    assert_eq!(request.schedule_cmd.activity_type, "c2_la");
    assert_eq!(request.schedule_cmd.attempt, 1);
    let original_schedule_time = request.schedule_cmd.original_schedule_time;
    println!("C2E2 LA_DISPATCH warm request={request:?}");
    let mut warm_execution_count = 0;
    let mut execute_la = || { warm_execution_count += 1; result.clone() };
    let held_result = execute_la();
    assert!(completion(&mut live, &mut history.events[3]).is_empty());
    live.wft_completion_reported();
    let mid_activation = live.new_work_from_server(
        update(&history.events[3..middle], 3, middle as i64), vec![],
    ).unwrap();
    activation("warm-middle", &live, &mid_activation, &out);
    assert!(!live.machines.replaying);
    assert_eq!(mid_activation.timestamp.as_ref().unwrap().seconds, T + 8);
    assert_eq!(mid_activation.history_length, middle as u32);
    if has_signal {
        assert_eq!(mid_activation.jobs.len(), 1);
        assert!(matches!(&mid_activation.jobs[0].variant,
            Some(Variant::SignalWorkflow(signal)) if signal.signal_name == "probe"));
        assert!(respond("warm-signal", &mut live, &mut warm_lang, mid_activation).is_empty());
        assert_eq!(warm_lang.signal_started, 1);
        assert_eq!(warm_lang.signal_completed, 1);
    } else {
        assert!(mid_activation.jobs.is_empty());
    }
    assert_eq!(live.machines.outstanding_local_activity_count(), 1);
    assert!(live.drain_queued_local_activities().is_empty());
    assert!(completion(&mut live, &mut history.events[middle]).is_empty());
    live.wft_completion_reported();
    let at_last = live.new_work_from_server(
        update(&history.events[middle..last], middle as i64, last as i64), vec![],
    ).unwrap();
    activation("warm-last-heartbeat", &live, &at_last, &out);
    assert!(at_last.jobs.is_empty());
    assert!(!live.machines.replaying);
    assert_eq!(at_last.timestamp.as_ref().unwrap().seconds, T + 16);
    assert_eq!(at_last.history_length, last as u32);
    assert_eq!(live.machines.outstanding_local_activity_count(), 1);
    assert!(live.drain_queued_local_activities().is_empty());
    println!("C2E2 LA_COMPLETE warm count={warm_execution_count} runtime=17");
    live.notify_of_local_result(LocalResolution::LocalActivity(LocalActivityResolution {
        seq: 1, result: LocalActivityExecutionResult::Completed(Success { result: Some(held_result) }),
        runtime: Duration::from_secs(17), attempt: 1, backoff: None, original_schedule_time,
    })).unwrap();
    let warm_resume = live.get_next_activation().unwrap();
    activation("warm-resume", &live, &warm_resume, &out);
    let warm_final_commands = respond("warm-resume", &mut live, &mut warm_lang, warm_resume.clone());
    let server_commands = completion(&mut live, &mut history.events[last]);
    let [marker_command, complete_command] = server_commands.as_slice() else {
        panic!("expected marker and completion");
    };
    let Some(CommandAttributes::RecordMarkerCommandAttributes(command_marker)) = &marker_command.attributes else {
        panic!("first command must record LA marker");
    };
    let Some(CommandAttributes::CompleteWorkflowExecutionCommandAttributes(command_complete)) = &complete_command.attributes else {
        panic!("second command must complete workflow");
    };
    let Some(Attributes::MarkerRecordedEventAttributes(marker)) = &mut history.events[marker_index].attributes else { unreachable!() };
    assert_eq!(marker.marker_name, command_marker.marker_name);
    assert_eq!(marker.failure, command_marker.failure);
    assert_eq!(marker.header, command_marker.header);
    marker.details = command_marker.details.clone();
    let (marker_data, marker_result) = extract_local_activity_marker_details(&mut marker.details.clone());
    let mut expected_marker = original_marker;
    expected_marker.original_schedule_time = original_schedule_time.map(Into::into);
    assert_eq!(marker_data.unwrap(), expected_marker);
    assert_eq!(marker_result, Some(result.clone()));
    let Some(Attributes::WorkflowExecutionCompletedEventAttributes(terminal)) = &history.events[terminal_index].attributes else { unreachable!() };
    assert_eq!(terminal.result, command_complete.result);
    validate_fixture(&history, has_signal);
    for (index, event) in history.events.iter().enumerate() {
        assert_eq!(event.event_time, original.events[index].event_time);
        if ![3, middle, last, marker_index].contains(&index) {
            assert_eq!(event, &original.events[index]);
        }
    }
    let encoded = history.encode_to_vec();
    save(&out.join(format!("{family}-runtime.pb")), &encoded);
    save(&out.join(format!("{family}-runtime.decoded.txt")), format!("{history:#?}\n").as_bytes());
    save(&out.join("warm.commands.txt"), format!("initial={warm_initial_commands:#?}\nfinal={warm_final_commands:#?}\nserver={server_commands:#?}\n").as_bytes());
    assert!(warm_lang.completed);
    assert!(live.machines.workflow_is_finished());
    assert_eq!(live.machines.outstanding_local_activity_count(), 0);
    assert_eq!(warm_execution_count, 1);
    println!("C2E2 WARM_NORMAL_COMPLETION commands accepted; no Server commit assertion");

    println!("C2E2 FAMILY {family} PATH cold");
    let replay_history = History::decode(encoded.as_slice()).unwrap();
    assert_eq!(replay_history, history);
    let mut cold = manager();
    let initial = cold.new_work_from_server(update(&replay_history.events, last as i64, last as i64), vec![]).unwrap();
    activation("cold-init", &cold, &initial, &out);
    let mut cold_lang = C2e2LanguageDriver::new(&initial, "c2-namespace", "c2_queue",
        result.clone(), workflow_result, None);
    let cold_initial_commands = respond("cold-init", &mut cold, &mut cold_lang, initial);
    assert_eq!(cold_initial_commands, warm_initial_commands);
    assert!(cold.drain_queued_local_activities().is_empty());
    assert_eq!(cold.machines.last_processed_event, 3);
    if has_signal {
        let signal_activation = cold.get_next_activation().unwrap();
        activation("cold-signal", &cold, &signal_activation, &out);
        assert_eq!(cold.machines.last_processed_event, 7);
        assert_eq!(cold.machines.get_last_wft_started_id(), 7);
        assert_eq!(signal_activation.jobs.len(), 1);
        assert!(matches!(&signal_activation.jobs[0].variant,
            Some(Variant::SignalWorkflow(signal)) if signal.signal_name == "probe"));
        assert_eq!(signal_activation.timestamp.as_ref().unwrap().seconds, T + 8);
        assert_eq!(signal_activation.history_length, 7);
        assert!(cold.drain_queued_local_activities().is_empty());
        assert!(respond("cold-signal", &mut cold, &mut cold_lang, signal_activation).is_empty());
        assert_eq!(cold_lang.signal_started, 1);
        assert_eq!(cold_lang.signal_completed, 1);
        assert!(!cold_lang.completed);
        assert_eq!(cold_lang.observations.borrow().len(), 2);
        println!("C2E2 COLD_SIGNAL boundary7 completed; LA still pending");
    }
    let cold_resume = cold.get_next_activation().unwrap();
    activation("cold-resume", &cold, &cold_resume, &out);
    assert_eq!(cold.machines.last_processed_event, last as i64);
    assert_eq!(cold.machines.get_last_wft_started_id(), last as i64);
    assert!(cold.drain_queued_local_activities().is_empty());
    let cold_final_commands = respond("cold-resume", &mut cold, &mut cold_lang, cold_resume.clone());
    assert_eq!(cold_final_commands, warm_final_commands);
    let tail = cold.get_next_activation().unwrap();
    activation("cold-terminal", &cold, &tail, &out);
    assert!(tail.jobs.is_empty());
    assert!(cold.machines.have_seen_terminal_event);
    assert_eq!(cold.machines.last_processed_event, (last + 3) as i64);
    let termination_requests = cold.drain_queued_local_activities();
    assert!(matches!(termination_requests.as_slice(), [LocalActRequest::CancelAllInRun(id)] if id == RUN_ID));
    assert!(cold_lang.completed);
    println!("C2E2 COLD_NORMAL_COMPLETION marker and terminal matched; only CancelAllInRun, no New LA request");
    for act in [&warm_resume, &cold_resume] {
        assert_eq!(act.jobs.len(), 1);
        let Some(Variant::ResolveActivity(resolution)) = &act.jobs[0].variant else { panic!("expected LA resolve"); };
        assert_eq!(resolution.seq, 1);
        assert!(resolution.is_local);
        assert_eq!(act.timestamp.as_ref().unwrap().seconds, T + 17);
        assert_eq!(act.timestamp.as_ref().unwrap().nanos, 0);
        assert_eq!(act.history_length, last as u32);
    }
    assert_eq!(warm_resume.jobs, cold_resume.jobs);
    assert!(!warm_resume.is_replaying);
    assert!(cold_resume.is_replaying);
    let warm_observations = warm_lang.observations.borrow();
    let cold_observations = cold_lang.observations.borrow();
    let expected_observations = if has_signal {
        vec![("before_la", T, 3, 0), ("probe", T + 8, 7, 1), ("after_la", T + 17, 10, 1)]
    } else {
        vec![("before_la", T, 3, 0), ("after_la", T + 17, 9, 0)]
    };
    for observations in [&*warm_observations, &*cold_observations] {
        assert_eq!(observations.len(), expected_observations.len());
        for (actual, &(stage, seconds, history_length, count)) in observations.iter().zip(&expected_observations) {
            assert_eq!(actual.stage, stage);
            assert_eq!(actual.workflow_time, UNIX_EPOCH + Duration::from_secs(seconds as u64));
            assert_eq!(actual.history_length, history_length);
            assert_eq!(actual.signal_receipt_count, count);
        }
    }
    assert!(warm_observations.iter().all(|o| !o.is_replaying));
    assert!(cold_observations.iter().all(|o| o.is_replaying));
    for lang in [&warm_lang, &cold_lang] {
        assert_eq!(lang.signal_started, u32::from(has_signal));
        assert_eq!(lang.signal_completed, u32::from(has_signal));
    }
    println!("C2E2 LANGUAGE_OBSERVATIONS warm={warm_observations:?} cold={cold_observations:?}");
    save(&out.join("language-observations.txt"), format!("warm={warm_observations:#?}\ncold={cold_observations:#?}\n").as_bytes());
    save(&out.join("cold.commands.txt"), format!("initial={cold_initial_commands:#?}\nfinal={cold_final_commands:#?}\n").as_bytes());
    println!("C2E2 RESULT family={family} compatibility=PASS signal_count_each={} warm_external_calls=1 cold_New_LA_requests=0", u32::from(has_signal));
}
