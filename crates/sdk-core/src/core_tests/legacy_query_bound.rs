//! A legacy query task is answered at the newest workflow task the server reports as completed
//! (`previous_started_event_id`). Events after that task's own completion and command events are
//! input no workflow task has processed yet, so preparing the query leaves them to the next
//! workflow task. These tests check what such a query observes, that the commands of that newest
//! completed task are still matched against history, and that the run is rebuilt from history
//! whenever preparing the query still has to run workflow code on such input.

use crate::{
    internal_flags::CoreInternalFlags,
    prost_dur,
    replay::TestHistoryBuilder,
    test_help::{
        MocksHolder, ResponseType, WorkerExt, hist_to_poll_resp, mock_worker, query_ok,
        schedule_activity_cmd, schedule_local_activity_cmd,
    },
    worker::client::{LegacyQueryResult, mocks::mock_worker_client},
};
use futures_util::{StreamExt, stream};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use temporalio_common::protos::{
    coresdk::{
        workflow_activation::{WorkflowActivation, workflow_activation_job::Variant as Job},
        workflow_commands::{
            ActivityCancellationType, ModifyWorkflowProperties, RequestCancelActivity, StartTimer,
            workflow_command,
        },
        workflow_completion::WorkflowActivationCompletion,
    },
    temporal::api::{
        command::v1::{Command, command::Attributes as CmdAttrs},
        common::v1::{ActivityType, Memo, Payload},
        enums::v1::EventType,
        history::v1::{
            ActivityTaskCanceledEventAttributes, History, HistoryEvent,
            TimerStartedEventAttributes, WorkflowPropertiesModifiedEventAttributes, history_event,
        },
        protocol::v1::Message as ProtocolMessage,
        query::v1::WorkflowQuery,
        workflowservice::v1::{
            GetWorkflowExecutionHistoryResponse, PollWorkflowTaskQueueResponse,
            RespondQueryTaskCompletedResponse,
        },
    },
};
use tokio::sync::Notify;

const WFID: &str = "legacy-query-bound";
const UPDATE_ID: &str = "upd";

/// What lang has observed. Dropped on eviction, as a real SDK drops the workflow instance.
#[derive(Default, Debug)]
struct Wf {
    acked: bool,
    signals: u32,
    resolved: Vec<u32>,
}

impl Wf {
    fn observe(&mut self, act: &WorkflowActivation) {
        for job in act.jobs.iter().filter_map(|j| j.variant.as_ref()) {
            match job {
                Job::InitializeWorkflow(_) => *self = Wf::default(),
                Job::ResolveSignalExternalWorkflow(_) => self.acked = true,
                Job::SignalWorkflow(_) => self.signals += 1,
                Job::ResolveActivity(r) => self.resolved.push(r.seq),
                _ => {}
            }
        }
    }
    fn answer(&self) -> String {
        format!(
            "acked={} signals={} resolved={:?}",
            self.acked, self.signals, self.resolved
        )
    }
}

const C_STATE: &str = "acked=false signals=0 resolved=[]";

type Cmds = fn(&WorkflowActivation) -> Vec<workflow_command::Variant>;
type Fetch = Arc<dyn Fn(usize, Vec<u8>) -> (Vec<HistoryEvent>, Vec<u8>) + Send + Sync>;

/// Starts a timer per signal, numbered by the history length its activation observed.
fn cmds_signal(act: &WorkflowActivation) -> Vec<workflow_command::Variant> {
    act.jobs
        .iter()
        .filter_map(|j| j.variant.as_ref())
        .filter(|j| matches!(j, Job::SignalWorkflow(_)))
        .map(|_| {
            StartTimer {
                seq: act.history_length,
                start_to_fire_timeout: Some(prost_dur!(from_secs(60))),
            }
            .into()
        })
        .collect()
}

/// Runs local activity 1 on start, then waits for a signal.
fn cmds_local_activity(act: &WorkflowActivation) -> Vec<workflow_command::Variant> {
    act.jobs
        .iter()
        .filter_map(|j| j.variant.as_ref())
        .filter(|j| matches!(j, Job::InitializeWorkflow(_)))
        .map(|_| {
            schedule_local_activity_cmd(
                1,
                "1",
                ActivityCancellationType::TryCancel,
                Duration::from_secs(60),
            )
        })
        .collect()
}

/// Schedules activity 1 on start; on a signal cancels it and runs local activity 2.
fn cmds_cancel_then_local_activity(act: &WorkflowActivation) -> Vec<workflow_command::Variant> {
    let mut cmds = vec![];
    for job in act.jobs.iter().filter_map(|j| j.variant.as_ref()) {
        match job {
            Job::InitializeWorkflow(_) => cmds.push(schedule_activity_cmd(
                1,
                "q",
                "1",
                ActivityCancellationType::TryCancel,
                Duration::from_secs(60),
                Duration::from_secs(60),
            )),
            Job::SignalWorkflow(_) => {
                cmds.push(RequestCancelActivity { seq: 1 }.into());
                cmds.push(schedule_local_activity_cmd(
                    2,
                    "2",
                    ActivityCancellationType::TryCancel,
                    Duration::from_secs(60),
                ));
            }
            _ => {}
        }
    }
    cmds
}

#[derive(Default, Debug)]
struct Observed {
    activations: Vec<Vec<String>>,
    query_history_lengths: Vec<u32>,
    query_answers: Vec<String>,
    completions: Vec<(Vec<Command>, Vec<ProtocolMessage>)>,
    wft_failures: Vec<String>,
    legacy_query_responses: usize,
    legacy_query_failures: Vec<String>,
    history_fetches: usize,
    timed_out: bool,
}

impl Observed {
    fn server_calls(&self) -> usize {
        self.completions.len() + self.wft_failures.len() + self.legacy_query_responses
    }
    fn position(&self, job: &str) -> Option<usize> {
        self.activations
            .iter()
            .position(|a| a.iter().any(|j| j == job))
    }
    fn count(&self, job: &str) -> usize {
        self.activations
            .iter()
            .map(|a| a.iter().filter(|j| *j == job).count())
            .sum()
    }
    fn evictions(&self) -> usize {
        self.count("Evict")
    }
    fn timers(&self) -> usize {
        self.completions
            .iter()
            .flat_map(|(cmds, _)| cmds)
            .filter(|c| matches!(c.attributes, Some(CmdAttrs::StartTimerCommandAttributes(_))))
            .count()
    }
}

#[derive(Clone)]
struct Server {
    obs: Arc<Mutex<Observed>>,
    answered: Arc<Notify>,
    all_answered: Arc<Notify>,
    expected_calls: usize,
}

impl Server {
    fn record(&self, f: impl FnOnce(&mut Observed)) {
        let mut obs = self.obs.lock().unwrap();
        f(&mut obs);
        if obs.server_calls() >= self.expected_calls {
            self.all_answered.notify_one();
        }
        self.answered.notify_one();
    }
}

async fn run(
    tasks: Vec<(bool, PollWorkflowTaskQueueResponse)>,
    cache: usize,
    cmds: Cmds,
    fetch: Option<Fetch>,
) -> Observed {
    let server = Server {
        obs: Arc::new(Mutex::new(Observed::default())),
        answered: Arc::new(Notify::new()),
        all_answered: Arc::new(Notify::new()),
        expected_calls: tasks.len(),
    };
    let mut client = mock_worker_client();
    {
        let server = server.clone();
        client
            .expect_complete_workflow_task()
            .returning(move |c, _| {
                server.record(|o| o.completions.push((c.commands, c.messages)));
                Ok(Default::default())
            });
    }
    {
        let server = server.clone();
        client
            .expect_respond_legacy_query()
            .returning(move |_, result| {
                server.record(|o| {
                    o.legacy_query_responses += 1;
                    if let LegacyQueryResult::Failed(f) = result {
                        o.legacy_query_failures
                            .push(f.failure.map(|f| f.message).unwrap_or_default());
                    }
                });
                Ok(RespondQueryTaskCompletedResponse::default())
            });
    }
    {
        let server = server.clone();
        client
            .expect_fail_workflow_task()
            .returning(move |_, _, failure| {
                let message = failure.map(|f| f.message).unwrap_or_default();
                server.record(|o| o.wft_failures.push(message));
                Ok(Default::default())
            });
    }
    if let Some(fetch) = fetch {
        let obs = server.obs.clone();
        client
            .expect_get_workflow_execution_history()
            .returning(move |_, _, token| {
                let call = {
                    let mut obs = obs.lock().unwrap();
                    obs.history_fetches += 1;
                    obs.history_fetches
                };
                let (events, next_page_token) = fetch(call, token);
                Ok(GetWorkflowExecutionHistoryResponse {
                    history: Some(History { events }),
                    next_page_token,
                    ..Default::default()
                })
            });
    }
    let answered = server.answered.clone();
    let tasks = stream::iter(tasks)
        .then(move |(wait, resp)| {
            let answered = answered.clone();
            async move {
                if wait {
                    answered.notified().await;
                }
                resp
            }
        })
        .chain(stream::pending());
    let mut mock = MocksHolder::from_wft_stream(client, tasks);
    mock.worker_cfg(|wc| wc.max_cached_workflows = cache);
    let core = mock_worker(mock);
    let mut wf = Wf::default();
    let driven = async {
        loop {
            let act = tokio::select! {
                biased;
                _ = server.all_answered.notified() => break,
                act = core.poll_workflow_activation() => match act {
                    Ok(act) => act,
                    Err(_) => break,
                },
            };
            let jobs: Vec<String> = act
                .jobs
                .iter()
                .filter_map(|j| j.variant.as_ref())
                .map(|job| {
                    match job {
                        Job::InitializeWorkflow(_) => "Init",
                        Job::ResolveSignalExternalWorkflow(_) => "ResolveSignalExternal",
                        Job::DoUpdate(_) => "DoUpdate",
                        Job::SignalWorkflow(_) => "Signal",
                        Job::QueryWorkflow(_) => "Query",
                        Job::RemoveFromCache(_) => "Evict",
                        Job::ResolveActivity(_) => "ResolveActivity",
                        Job::NotifyHasPatch(_) => "NotifyHasPatch",
                        _ => "Other",
                    }
                    .to_string()
                })
                .collect();
            let completion = if jobs.iter().any(|j| j == "Evict") {
                wf = Wf::default();
                WorkflowActivationCompletion::empty(act.run_id.clone())
            } else if jobs.iter().any(|j| j == "Query") {
                let answer = wf.answer();
                let mut obs = server.obs.lock().unwrap();
                obs.query_answers.push(answer.clone());
                obs.query_history_lengths.push(act.history_length);
                let responses = act
                    .jobs
                    .iter()
                    .filter_map(|j| match j.variant.as_ref() {
                        Some(Job::QueryWorkflow(q)) => {
                            Some(query_ok(q.query_id.clone(), answer.as_str()))
                        }
                        _ => None,
                    })
                    .collect();
                WorkflowActivationCompletion::from_cmds(act.run_id.clone(), responses)
            } else {
                wf.observe(&act);
                WorkflowActivationCompletion::from_cmds(act.run_id.clone(), cmds(&act))
            };
            {
                let mut obs = server.obs.lock().unwrap();
                obs.activations.push(jobs);
            }
            if core.complete_workflow_activation(completion).await.is_err() {
                break;
            }
        }
    };
    if tokio::time::timeout(Duration::from_secs(10), driven)
        .await
        .is_err()
    {
        server.obs.lock().unwrap().timed_out = true;
    }
    if tokio::time::timeout(Duration::from_secs(10), core.drain_pollers_and_shutdown())
        .await
        .is_err()
    {
        server.obs.lock().unwrap().timed_out = true;
    }
    std::mem::take(&mut *server.obs.lock().unwrap())
}

fn poll(t: &TestHistoryBuilder) -> PollWorkflowTaskQueueResponse {
    hist_to_poll_resp(t, WFID, ResponseType::AllHistory).resp
}

/// A non-sticky legacy query as the server builds it at poll time.
fn legacy_query(t: &TestHistoryBuilder, previous_started: i64) -> PollWorkflowTaskQueueResponse {
    let mut query = poll(t);
    query.query = Some(WorkflowQuery {
        query_type: "state".to_owned(),
        ..Default::default()
    });
    query.started_event_id = 0;
    query.previous_started_event_id = previous_started;
    query
}

fn full_fetch(t: &TestHistoryBuilder) -> Fetch {
    let events = t.get_full_history_info().unwrap().into_events();
    Arc::new(move |_, _| (events.clone(), vec![]))
}

/// Appends the events the server records for a workflow task completion, in command order.
fn commit(
    t: &mut TestHistoryBuilder,
    commands: &[Command],
    messages: &[ProtocolMessage],
) -> Result<(), String> {
    t.add_workflow_task_completed();
    let completed_id = t.current_event_id();
    let mut accepted_id = 0;
    for c in commands {
        match &c.attributes {
            Some(CmdAttrs::ModifyWorkflowPropertiesCommandAttributes(a)) => {
                t.add(WorkflowPropertiesModifiedEventAttributes {
                    workflow_task_completed_event_id: completed_id,
                    upserted_memo: a.upserted_memo.clone(),
                });
            }
            Some(CmdAttrs::StartTimerCommandAttributes(a)) => {
                t.add(TimerStartedEventAttributes {
                    timer_id: a.timer_id.clone(),
                    start_to_fire_timeout: a.start_to_fire_timeout,
                    workflow_task_completed_event_id: completed_id,
                });
            }
            Some(CmdAttrs::ProtocolMessageCommandAttributes(a)) => {
                let msg = messages
                    .iter()
                    .find(|m| m.id == a.message_id)
                    .expect("protocol message command must reference a sent message");
                if msg.id.ends_with("/accept") {
                    accepted_id = t.add_update_accepted(UPDATE_ID, "update_fn");
                } else {
                    t.add_update_completed(accepted_id);
                }
            }
            other => return Err(format!("unexpected live command {other:?}")),
        }
    }
    t.add_workflow_task_scheduled_and_started();
    Ok(())
}

/// Commits the last completion and replays the result on a fresh worker with stock settings.
async fn cold_replay(mut history: TestHistoryBuilder, live: &Observed, cmds: Cmds) -> Observed {
    let Some((c, m)) = live.completions.last().cloned() else {
        return Observed::default();
    };
    if let Err(e) = commit(&mut history, &c, &m) {
        return Observed {
            wft_failures: vec![e],
            ..Default::default()
        };
    }
    run(vec![(false, poll(&history))], 1, cmds, None).await
}

fn start_timer(seq: u32) -> workflow_command::Variant {
    StartTimer {
        seq,
        start_to_fire_timeout: Some(prost_dur!(from_secs(60))),
    }
    .into()
}

fn on_init(
    act: &WorkflowActivation,
    f: impl Fn() -> Vec<workflow_command::Variant>,
) -> Vec<workflow_command::Variant> {
    if act
        .jobs
        .iter()
        .any(|j| matches!(j.variant, Some(Job::InitializeWorkflow(_))))
    {
        f()
    } else {
        vec![]
    }
}

fn query_failed_nondeterministically(live: &Observed) -> bool {
    live.legacy_query_failures
        .iter()
        .any(|f| f.contains("TMPRL1100"))
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum NextTask {
    FullHistory,
    Incremental,
}

/// 1 started, 2-3 WFT1, 4 completed, 5 signal, 6 WFT2 scheduled, then the legacy query, then 7
/// WFT2 started. The query is dispatched after WFT1 completed and polled after the signal arrived.
#[rstest::rstest]
#[tokio::test]
async fn query_does_not_observe_input_no_task_processed(
    #[values(0, 1)] cache: usize,
    #[values(NextTask::FullHistory, NextTask::Incremental)] next: NextTask,
) {
    let mut t = TestHistoryBuilder::default();
    t.add_by_type(EventType::WorkflowExecutionStarted);
    t.add_workflow_task_scheduled_and_started();
    let first = poll(&t);
    t.add_workflow_task_completed();
    if cfg!(feature = "disable_workflow_task_chunking") {
        t.set_flags_first_wft(&[CoreInternalFlags::DisableWorkflowTaskChunking], &[]);
    }
    t.add_we_signaled("S", vec![]);
    t.add_workflow_task_scheduled();
    let query = legacy_query(&t, 3);
    t.add_workflow_task_started();
    let wft = match next {
        NextTask::FullHistory => poll(&t),
        NextTask::Incremental => hist_to_poll_resp(&t, WFID, ResponseType::OneTask(2)).resp,
    };
    let tasks = vec![(false, first), (true, query), (true, wft)];
    let live = run(tasks, cache, cmds_signal, Some(full_fetch(&t))).await;
    let replay = cold_replay(t, &live, cmds_signal).await;

    assert!(!live.timed_out);
    assert_eq!(live.legacy_query_responses, 1);
    assert!(live.wft_failures.is_empty(), "{:?}", live.wft_failures);
    assert!(replay.wft_failures.is_empty(), "{:?}", replay.wft_failures);
    assert_eq!(live.query_answers, [C_STATE]);
    // WFT1 recorded no commands, so nothing after its started event is applied.
    assert_eq!(live.query_history_lengths, [3]);
    assert!(live.position("Query").unwrap() < live.position("Signal").unwrap());
    assert_eq!(live.timers(), 1, "the signal is handled exactly once");
    if cache == 1 {
        assert_eq!(live.evictions(), 0);
        assert_eq!(live.count("Signal"), 1);
        assert_eq!(live.history_fetches, 0);
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum TimerCode {
    Matching,
    Missing,
    Extra,
    WrongType,
    WrongId,
}

fn timer_code(code: TimerCode) -> Cmds {
    fn matching(act: &WorkflowActivation) -> Vec<workflow_command::Variant> {
        on_init(act, || vec![start_timer(1)])
    }
    fn missing(act: &WorkflowActivation) -> Vec<workflow_command::Variant> {
        on_init(act, Vec::new)
    }
    fn extra(act: &WorkflowActivation) -> Vec<workflow_command::Variant> {
        on_init(act, || vec![start_timer(1), start_timer(2)])
    }
    fn wrong_type(act: &WorkflowActivation) -> Vec<workflow_command::Variant> {
        on_init(act, || {
            vec![schedule_activity_cmd(
                1,
                "q",
                "1",
                ActivityCancellationType::TryCancel,
                Duration::from_secs(60),
                Duration::from_secs(60),
            )]
        })
    }
    fn wrong_id(act: &WorkflowActivation) -> Vec<workflow_command::Variant> {
        on_init(act, || vec![start_timer(2)])
    }
    match code {
        TimerCode::Matching => matching,
        TimerCode::Missing => missing,
        TimerCode::Extra => extra,
        TimerCode::WrongType => wrong_type,
        TimerCode::WrongId => wrong_id,
    }
}

/// The newest completed task (started at 3) recorded `TimerStarted` at 5. Its command events sit
/// after its started event, in the same history sequence as the signal no task has processed, yet
/// they must still be matched. Extra commands and timer ids were never caught for the last task in
/// history, so they still are not.
#[rstest::rstest]
#[tokio::test]
async fn query_matches_commands_of_newest_completed_task(
    #[values(
        TimerCode::Matching,
        TimerCode::Missing,
        TimerCode::Extra,
        TimerCode::WrongType,
        TimerCode::WrongId
    )]
    code: TimerCode,
    #[values(false, true)] trailing_signal: bool,
) {
    let mut t = TestHistoryBuilder::default();
    t.add_by_type(EventType::WorkflowExecutionStarted);
    t.add_full_wf_task();
    t.add_timer_started("1".to_owned());
    if trailing_signal {
        t.add_we_signaled("S", vec![]);
        t.add_workflow_task_scheduled();
    }
    let live = run(
        vec![(false, legacy_query(&t, 3))],
        0,
        timer_code(code),
        None,
    )
    .await;

    assert_eq!(live.legacy_query_responses, 1);
    assert_eq!(live.count("Signal"), 0, "the trailing signal must not run");
    let expect_nondeterminism = matches!(code, TimerCode::Missing | TimerCode::WrongType);
    assert_eq!(
        query_failed_nondeterministically(&live),
        expect_nondeterminism,
        "{:?}",
        live.legacy_query_failures
    );
    if !expect_nondeterminism {
        assert!(live.legacy_query_failures.is_empty());
        assert_eq!(live.query_history_lengths, [5]);
    }
}

fn la_code(schedules: bool) -> Cmds {
    fn yes(act: &WorkflowActivation) -> Vec<workflow_command::Variant> {
        cmds_local_activity(act)
    }
    fn no(act: &WorkflowActivation) -> Vec<workflow_command::Variant> {
        on_init(act, Vec::new)
    }
    if schedules { yes } else { no }
}

/// The newest completed task recorded a local activity marker. The query sees the recorded result
/// and fails if the code no longer runs that local activity.
#[rstest::rstest]
#[tokio::test]
async fn query_matches_local_activity_marker_of_newest_completed_task(
    #[values(true, false)] code_matches: bool,
    #[values(false, true)] paginated: bool,
) {
    let mut t = TestHistoryBuilder::default();
    t.add_by_type(EventType::WorkflowExecutionStarted);
    t.add_full_wf_task();
    t.add_local_activity_result_marker(1, "1", Payload::default());
    t.add_we_signaled("S", vec![]);
    t.add_workflow_task_scheduled();
    let mut query = legacy_query(&t, 3);
    let mut fetch = None;
    if paginated {
        // The first page ends at the newest completed task's started event; the second holds its
        // completion, the marker and the pending signal.
        let full = t.get_full_history_info().unwrap().into_events();
        query.history = Some(History {
            events: full[..3].to_vec(),
        });
        query.next_page_token = b"page-2".to_vec();
        let rest = full[3..].to_vec();
        fetch = Some(Arc::new(move |_, _| (rest.clone(), vec![])) as Fetch);
    }
    let live = run(vec![(false, query)], 0, la_code(code_matches), fetch).await;

    assert_eq!(live.legacy_query_responses, 1);
    assert_eq!(live.history_fetches, usize::from(paginated));
    if code_matches {
        assert!(live.legacy_query_failures.is_empty());
        assert_eq!(live.query_answers, ["acked=false signals=0 resolved=[1]"]);
    } else {
        assert!(
            query_failed_nondeterministically(&live),
            "{:?}",
            live.legacy_query_failures
        );
    }
}

/// Cancels activity 1 on the signal, without running local activity 2.
fn cmds_cancel_only(act: &WorkflowActivation) -> Vec<workflow_command::Variant> {
    let mut cmds = cmds_cancel_then_local_activity(act);
    cmds.retain(|c| !matches!(c, workflow_command::Variant::ScheduleLocalActivity(_)));
    cmds
}

/// The newest completed task (started at 8) cancelled an activity that had not started, so the
/// server recorded `ActivityTaskCanceled` among that task's command events, before local activity
/// 2's marker. Matching that marker requires applying the cancellation too.
#[rstest::rstest]
#[tokio::test]
async fn query_matches_commands_after_interleaved_cancellation(
    #[values(true, false)] code_matches: bool,
) {
    let mut t = TestHistoryBuilder::default();
    t.add_by_type(EventType::WorkflowExecutionStarted);
    t.add_full_wf_task();
    let scheduled = t.add_activity_task_scheduled("1");
    t.add_we_signaled("S", vec![]);
    t.add_full_wf_task();
    t.add_activity_task_cancel_requested(scheduled);
    let cancel_requested = t.current_event_id();
    t.add(ActivityTaskCanceledEventAttributes {
        scheduled_event_id: scheduled,
        latest_cancel_requested_event_id: cancel_requested,
        ..Default::default()
    });
    t.add_local_activity_result_marker(2, "2", Payload::default());
    t.add_we_signaled("S2", vec![]);
    t.add_workflow_task_scheduled();
    let cmds: Cmds = if code_matches {
        cmds_cancel_then_local_activity
    } else {
        cmds_cancel_only
    };
    let live = run(vec![(false, legacy_query(&t, 8))], 0, cmds, None).await;

    assert_eq!(live.legacy_query_responses, 1);
    if code_matches {
        assert!(live.legacy_query_failures.is_empty());
        assert_eq!(
            live.query_answers,
            ["acked=false signals=1 resolved=[1, 2]"]
        );
        assert_eq!(live.query_history_lengths, [12]);
    } else {
        assert!(
            query_failed_nondeterministically(&live),
            "{:?}",
            live.legacy_query_failures
        );
    }
}

/// Schedules activity 1, waiting for its cancellation to complete, on start. Signal "S" cancels
/// it and starts timer 1, any other signal starts timer 3. With `order_sensitive`, resolving the
/// activity records a memo while history is shorter than the started event of the task that
/// delivers the resolution live (14), and starts timer 2 otherwise.
fn wait_cancel_cmds(
    act: &WorkflowActivation,
    order_sensitive: bool,
) -> Vec<workflow_command::Variant> {
    let mut cmds = vec![];
    for job in act.jobs.iter().filter_map(|j| j.variant.as_ref()) {
        match job {
            Job::InitializeWorkflow(_) => cmds.push(schedule_activity_cmd(
                1,
                "q",
                "1",
                ActivityCancellationType::WaitCancellationCompleted,
                Duration::from_secs(60),
                Duration::from_secs(60),
            )),
            Job::SignalWorkflow(s) if s.signal_name == "S" => {
                cmds.push(RequestCancelActivity { seq: 1 }.into());
                cmds.push(start_timer(1));
            }
            Job::SignalWorkflow(_) => cmds.push(start_timer(3)),
            Job::ResolveActivity(_) if order_sensitive && act.history_length < 14 => cmds.push(
                ModifyWorkflowProperties {
                    upserted_memo: Some(Memo {
                        fields: HashMap::from([("progress".to_string(), Payload::default())]),
                    }),
                }
                .into(),
            ),
            Job::ResolveActivity(_) => cmds.push(start_timer(2)),
            _ => {}
        }
    }
    cmds
}

fn wait_cancel_plain(act: &WorkflowActivation) -> Vec<workflow_command::Variant> {
    wait_cancel_cmds(act, false)
}

fn wait_cancel_order_sensitive(act: &WorkflowActivation) -> Vec<workflow_command::Variant> {
    wait_cancel_cmds(act, true)
}

/// 1-3 WFT1 schedules activity 1; 4 completed, 5 scheduled, 6 signal "S", 7-8 WFT2; 9 completed,
/// 10 cancel requested, 11 `ActivityTaskCanceled`, 12 timer 1 started, 13 WFT3 scheduled.
fn resolving_cancel_history() -> (TestHistoryBuilder, PollWorkflowTaskQueueResponse) {
    let mut t = TestHistoryBuilder::default();
    t.add_by_type(EventType::WorkflowExecutionStarted);
    t.add_workflow_task_scheduled_and_started();
    let first = poll(&t);
    t.add_workflow_task_completed();
    let scheduled = t.add_activity_task_scheduled("1");
    t.add_we_signaled("S", vec![]);
    t.add_workflow_task_scheduled_and_started();
    t.add_workflow_task_completed();
    t.add_activity_task_cancel_requested(scheduled);
    let cancel_requested = t.current_event_id();
    t.add(ActivityTaskCanceledEventAttributes {
        scheduled_event_id: scheduled,
        latest_cancel_requested_event_id: cancel_requested,
        ..Default::default()
    });
    t.add_timer_started("1".to_owned());
    t.add_workflow_task_scheduled();
    (t, first)
}

/// The newest completed task (started at 8) cancelled an activity that had not started and waits
/// for the cancellation to complete. The `ActivityTaskCanceled` the server recorded among that
/// task's command events resolves the activity, and it must be applied to match the timer after
/// it, so workflow code runs and the query observes the resolution. The run is rebuilt before the
/// next task, which delivers the resolution at the boundary replay uses.
#[rstest::rstest]
#[tokio::test]
async fn interleaved_resolution_during_query_rebuilds_run(
    #[values(false, true)] order_sensitive: bool,
) {
    let cmds: Cmds = if order_sensitive {
        wait_cancel_order_sensitive
    } else {
        wait_cancel_plain
    };
    let (mut t, first) = resolving_cancel_history();
    let second = hist_to_poll_resp(&t, WFID, ResponseType::OneTask(2)).resp;
    let query = legacy_query(&t, 8);
    t.add_workflow_task_started();
    let wft = poll(&t);
    let tasks = vec![(false, first), (true, second), (true, query), (true, wft)];
    let live = run(tasks, 1, cmds, None).await;
    let replay = cold_replay(t, &live, cmds).await;

    assert_eq!(live.legacy_query_responses, 1);
    assert!(live.wft_failures.is_empty(), "{:?}", live.wft_failures);
    assert!(replay.wft_failures.is_empty(), "{:?}", replay.wft_failures);
    assert_eq!(live.query_answers, ["acked=false signals=1 resolved=[1]"]);
    assert!(live.position("ResolveActivity").unwrap() < live.position("Query").unwrap());
    assert_eq!(live.evictions(), 1);
    assert!(live.position("Query").unwrap() < live.position("Evict").unwrap());
}

/// Like [interleaved_resolution_during_query_rebuilds_run], but the history Core applies already
/// holds the next task (13-16), completed after the query was polled. The cancellation is then
/// applied from a history sequence that is not the last one, and the run must still be rebuilt.
///
/// `sticky`: a sticky query reaches a worker without the run, which fetches the history. Otherwise
/// the query carries that history itself.
#[rstest::rstest]
#[tokio::test]
async fn interleaved_resolution_with_later_completed_task_rebuilds_run(
    #[values(true, false)] sticky: bool,
    #[values(false, true)] order_sensitive: bool,
) {
    let cmds: Cmds = if order_sensitive {
        wait_cancel_order_sensitive
    } else {
        wait_cancel_plain
    };
    let (mut t, first) = resolving_cancel_history();
    t.add_workflow_task_started();
    t.add_workflow_task_completed();
    t.add_timer_started("2".to_owned());
    let mut query = legacy_query(&t, 8);
    if sticky {
        query.history = Some(History { events: vec![] });
    }
    t.add_we_signaled("S2", vec![]);
    t.add_workflow_task_scheduled_and_started();
    let wft = poll(&t);
    let tasks = if sticky {
        vec![(false, query), (true, wft)]
    } else {
        vec![(false, first), (true, query), (true, wft)]
    };
    let live = run(tasks, 1, cmds, Some(full_fetch(&t))).await;
    let replay = cold_replay(t, &live, cmds).await;

    assert_eq!(live.legacy_query_responses, 1);
    assert!(!live.timed_out);
    assert!(live.wft_failures.is_empty(), "{:?}", live.wft_failures);
    assert!(replay.wft_failures.is_empty(), "{:?}", replay.wft_failures);
    assert_eq!(live.query_answers, ["acked=false signals=1 resolved=[1]"]);
    assert!(live.position("ResolveActivity").unwrap() < live.position("Query").unwrap());
    assert_eq!(live.evictions(), 1);
    assert_eq!(live.timers(), 1, "only the second signal's timer is new");
}

/// Schedules activity 1 on start; signal "S" starts timer 1 and any other signal starts timer 2.
fn flag_gated_cmds(act: &WorkflowActivation) -> Vec<workflow_command::Variant> {
    let mut cmds = vec![];
    for job in act.jobs.iter().filter_map(|j| j.variant.as_ref()) {
        match job {
            Job::InitializeWorkflow(_) => cmds.push(schedule_activity_cmd(
                1,
                "q",
                "1",
                ActivityCancellationType::TryCancel,
                Duration::from_secs(60),
                Duration::from_secs(60),
            )),
            Job::SignalWorkflow(s) if s.signal_name == "S" => cmds.push(start_timer(1)),
            Job::SignalWorkflow(_) => cmds.push(start_timer(2)),
            _ => {}
        }
    }
    cmds
}

/// The newest completed task (started at 3) recorded an activity type the code no longer uses.
/// Only the id and type determinism check catches that, and it applies when history recorded the
/// flag for it or when the event belongs to the last history sequence. Matching the task's command
/// events for a query must not change which of those holds.
///
/// `stale`: the history also holds the next task (started at 8), completed after the query was
/// polled, so the command event is no longer in the last history sequence.
#[rstest::rstest]
#[tokio::test]
async fn id_and_type_check_on_newest_completed_task_follows_flag_rules(
    #[values(false, true)] stale: bool,
    #[values(false, true)] flag_recorded: bool,
) {
    let mut t = TestHistoryBuilder::default();
    t.add_by_type(EventType::WorkflowExecutionStarted);
    t.add_workflow_task_scheduled_and_started();
    t.add_workflow_task_completed();
    if flag_recorded {
        t.set_flags_first_wft(&[CoreInternalFlags::IdAndTypeDeterminismChecks], &[]);
    }
    t.add_activity_task_scheduled("1");
    if let Some(history_event::Attributes::ActivityTaskScheduledEventAttributes(a)) =
        t.last_event().and_then(|e| e.attributes.as_mut())
    {
        a.activity_type = Some(ActivityType {
            name: "recorded_by_other_code".to_owned(),
        });
    }
    t.add_we_signaled("S", vec![]);
    t.add_workflow_task_scheduled();
    let mut query = legacy_query(&t, 3);
    if stale {
        t.add_workflow_task_started();
        t.add_workflow_task_completed();
        t.add_timer_started("1".to_owned());
        query = legacy_query(&t, 3);
        t.add_we_signaled("S2", vec![]);
        t.add_workflow_task_scheduled_and_started();
    } else {
        t.add_workflow_task_started();
    }
    let wft = poll(&t);
    let live = run(vec![(false, query), (true, wft)], 1, flag_gated_cmds, None).await;

    assert_eq!(live.legacy_query_responses, 1);
    assert_eq!(
        query_failed_nondeterministically(&live),
        flag_recorded || !stale,
        "{:?}",
        live.legacy_query_failures
    );
}
