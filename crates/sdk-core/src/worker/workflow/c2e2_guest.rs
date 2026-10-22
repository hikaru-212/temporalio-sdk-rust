use futures_util::task::noop_waker_ref;
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, SystemTime},
};
use temporalio_common::{
    UntypedActivity,
    data_converters::{DataConverter, RawValue},
    protos::{
        coresdk::{
            workflow_activation::{WorkflowActivation, workflow_activation_job::Variant},
            workflow_commands::{CompleteWorkflowExecution, WorkflowCommand, workflow_command},
        },
        temporal::api::common::v1::Payload,
    },
};
use temporalio_workflow::{
    ActivityCancellationType, BaseWorkflowContext, LocalActivityOptions, WorkflowContext,
    WorkflowResult,
    __private::sdk::{
        ActivationJobResult, GuestWorkflowInstance, MAIN_ROUTINE_ID, MainRoutineCompletion,
        RoutineCompletion, RoutineId, RoutineKind, RoutinePendingState, TerminalOutcome,
        WorkflowHost, WorkflowInit, WorkflowInstance,
    },
    workflow, workflow_methods,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct C2e2LanguageObservation {
    pub(super) stage: &'static str,
    pub(super) workflow_time: SystemTime,
    pub(super) history_length: u32,
    pub(super) is_replaying: bool,
    pub(super) signal_receipt_count: u32,
}

#[workflow]
#[derive(Default)]
struct C2e2SignalWorkflow {
    observations: Rc<RefCell<Vec<C2e2LanguageObservation>>>,
    expected_la_result: Payload,
    workflow_result: Payload,
    original_schedule_time: Option<SystemTime>,
    signal_receipt_count: u32,
}

#[workflow_methods]
impl C2e2SignalWorkflow {
    #[run]
    async fn run(ctx: &mut WorkflowContext<Self>) -> WorkflowResult<RawValue> {
        let observations = ctx.state(|workflow| workflow.observations.clone());
        observations.borrow_mut().push(C2e2LanguageObservation {
            stage: "before_la",
            workflow_time: ctx.workflow_time().expect("workflow clock initialized"),
            history_length: ctx.history_length(),
            is_replaying: ctx.is_replaying(),
            signal_receipt_count: ctx.state(|workflow| workflow.signal_receipt_count),
        });
        let mut options = LocalActivityOptions::default();
        options.activity_id = Some("la-1".to_owned());
        options.schedule_to_close_timeout = Some(Duration::from_secs(60));
        options.start_to_close_timeout = Some(Duration::from_secs(60));
        options.cancel_type = ActivityCancellationType::WaitCancellationCompleted;
        options.original_schedule_time = ctx
            .state(|workflow| workflow.original_schedule_time)
            .map(Into::into);
        let result = ctx
            .execute_local_activity(
                UntypedActivity::new("c2_la"),
                RawValue::new(vec![]),
                options,
            )
            .await?;
        assert_eq!(
            result.payloads,
            vec![ctx.state(|workflow| workflow.expected_la_result.clone())],
            "language received the frozen local activity result",
        );
        observations.borrow_mut().push(C2e2LanguageObservation {
            stage: "after_la",
            workflow_time: ctx.workflow_time().expect("workflow clock initialized"),
            history_length: ctx.history_length(),
            is_replaying: ctx.is_replaying(),
            signal_receipt_count: ctx.state(|workflow| workflow.signal_receipt_count),
        });
        Ok(RawValue::new(vec![
            ctx.state(|workflow| workflow.workflow_result.clone()),
        ]))
    }

    #[signal]
    async fn probe(ctx: &mut WorkflowContext<Self>) {
        let workflow_time = ctx.workflow_time().expect("workflow clock initialized");
        let history_length = ctx.history_length();
        let is_replaying = ctx.is_replaying();
        ctx.state_mut(|workflow| {
            workflow.signal_receipt_count += 1;
            workflow.observations.borrow_mut().push(C2e2LanguageObservation {
                stage: "probe",
                workflow_time,
                history_length,
                is_replaying,
                signal_receipt_count: workflow.signal_receipt_count,
            });
        });
    }
}

#[derive(Default)]
struct C2e2LanguageHost {
    commands: RefCell<Vec<WorkflowCommand>>,
}

impl WorkflowHost for C2e2LanguageHost {
    fn set_current_details(&self, _details: String) {}

    fn push_command(&self, command: WorkflowCommand) {
        self.commands.borrow_mut().push(command);
    }
}

pub(super) struct C2e2LanguageDriver {
    instance: GuestWorkflowInstance<C2e2SignalWorkflow>,
    host: Rc<C2e2LanguageHost>,
    pub(super) observations: Rc<RefCell<Vec<C2e2LanguageObservation>>>,
    workflow_result: Payload,
    active_routines: Vec<RoutineId>,
    pub(super) completed: bool,
    pub(super) signal_started: u32,
    pub(super) signal_completed: u32,
}

impl C2e2LanguageDriver {
    pub(super) fn new(
        initial_activation: &WorkflowActivation,
        namespace: &str,
        task_queue: &str,
        expected_la_result: Payload,
        workflow_result: Payload,
        original_schedule_time: Option<SystemTime>,
    ) -> Self {
        let initialize_workflow = initial_activation
            .jobs
            .iter()
            .find_map(|job| match &job.variant {
                Some(Variant::InitializeWorkflow(init)) => Some(init.clone()),
                _ => None,
            })
            .expect("initial activation contains InitializeWorkflow");
        assert!(
            initialize_workflow.arguments.is_empty(),
            "the registered workflow has no input",
        );
        let host = Rc::new(C2e2LanguageHost::default());
        let observations = Rc::new(RefCell::new(vec![]));
        let base_ctx = BaseWorkflowContext::from_raw(
            WorkflowInit {
                namespace: namespace.to_owned(),
                task_queue: task_queue.to_owned(),
                run_id: initial_activation.run_id.clone(),
                initialize_workflow,
            },
            DataConverter::default(),
            host.clone(),
            None,
            vec![],
        );
        let instance = GuestWorkflowInstance::new_with_workflow(
            C2e2SignalWorkflow {
                observations: observations.clone(),
                expected_la_result,
                workflow_result: workflow_result.clone(),
                original_schedule_time,
                signal_receipt_count: 0,
            },
            base_ctx,
            Some(()),
        );
        Self {
            instance,
            host,
            observations,
            workflow_result,
            active_routines: vec![],
            completed: false,
            signal_started: 0,
            signal_completed: 0,
        }
    }

    pub(super) fn activate(&mut self, activation: WorkflowActivation) -> Vec<WorkflowCommand> {
        assert!(!self.completed, "no workflow activation follows completion");
        assert!(!activation.jobs.is_empty(), "jobless heartbeat stays inside Core");
        let signal_jobs: Vec<Option<String>> = activation
            .jobs
            .iter()
            .map(|job| match &job.variant {
                Some(Variant::InitializeWorkflow(_) | Variant::ResolveActivity(_)) => None,
                Some(Variant::SignalWorkflow(signal)) => {
                    assert_eq!(signal.signal_name, "probe");
                    Some(signal.signal_name.clone())
                }
                other => panic!("INCONCLUSIVE: unsupported activation job {other:?}"),
            })
            .collect();
        let replaying = activation.is_replaying;
        let history_length = activation.history_length;
        assert!(self.host.commands.borrow().is_empty());
        let activation_result = self.instance.activate(activation, noop_waker_ref()).unwrap();
        println!(
            "C2E2_GUEST activate replay={replaying} history_length={history_length} results={activation_result:?}",
        );
        assert_eq!(signal_jobs.len(), activation_result.job_results.len());
        for (signal_name, result) in signal_jobs.into_iter().zip(activation_result.job_results) {
            match (signal_name, result) {
                (None, ActivationJobResult::None) => {}
                (Some(expected_name), ActivationJobResult::StartedRoutine(started)) => {
                    assert_eq!(started.kind, RoutineKind::Signal(expected_name));
                    assert_ne!(started.routine_id, MAIN_ROUTINE_ID);
                    assert!(!self.active_routines.contains(&started.routine_id));
                    self.signal_started += 1;
                    println!("C2E2_GUEST signal_started {started:?}");
                    self.active_routines.push(started.routine_id);
                }
                unexpected => panic!("INCONCLUSIVE: unexpected activation result {unexpected:?}"),
            }
        }
        let mut settled = false;
        for pass in 0..16 {
            let mut pass_made_progress = false;
            let mut still_active = vec![];
            for routine_id in std::mem::take(&mut self.active_routines) {
                let command_count = self.host.commands.borrow().len();
                let result = self.instance.poll_routine(routine_id, noop_waker_ref()).unwrap();
                println!("C2E2_GUEST signal_poll pass={pass} routine={routine_id} result={result:?}");
                assert_eq!(
                    self.host.commands.borrow().len(),
                    command_count,
                    "probe signal handler must not issue workflow commands",
                );
                assert_ne!(
                    result.pending_state,
                    Some(RoutinePendingState::Interceptor),
                    "INCONCLUSIVE: adapter cannot settle an interceptor awaiting an external wake",
                );
                pass_made_progress |= result.made_progress;
                match result.completion {
                    None => still_active.push(routine_id),
                    Some(RoutineCompletion::Signal(Ok(()))) => {
                        self.signal_completed += 1;
                        println!("C2E2_GUEST signal_completed routine={routine_id}");
                    }
                    unexpected => panic!("INCONCLUSIVE: unexpected signal result {unexpected:?}"),
                }
            }
            self.active_routines = still_active;
            let result = self.instance.poll_routine(MAIN_ROUTINE_ID, noop_waker_ref()).unwrap();
            println!("C2E2_GUEST main_poll pass={pass} result={result:?}");
            assert_ne!(
                result.pending_state,
                Some(RoutinePendingState::Interceptor),
                "INCONCLUSIVE: adapter cannot settle an interceptor awaiting an external wake",
            );
            pass_made_progress |= result.made_progress;
            match result.completion {
                Some(RoutineCompletion::Main(MainRoutineCompletion::Blocked)) => {
                    if !pass_made_progress {
                        assert!(
                            self.active_routines.is_empty(),
                            "INCONCLUSIVE: the non-awaiting probe signal routine did not complete",
                        );
                        settled = true;
                        break;
                    }
                }
                Some(RoutineCompletion::Main(MainRoutineCompletion::Terminal(outcome))) => {
                    let TerminalOutcome::Completed(payload) = *outcome else {
                        panic!("workflow returned an unexpected terminal outcome");
                    };
                    assert_eq!(payload, self.workflow_result);
                    assert!(self.active_routines.is_empty());
                    assert_eq!(self.signal_started, self.signal_completed);
                    self.host.push_command(
                        workflow_command::Variant::CompleteWorkflowExecution(
                            CompleteWorkflowExecution { result: Some(payload) },
                        )
                        .into(),
                    );
                    self.completed = true;
                    settled = true;
                    break;
                }
                unexpected => panic!("INCONCLUSIVE: unexpected main routine result {unexpected:?}"),
            }
        }
        assert!(
            settled,
            "INCONCLUSIVE: language adapter did not settle within its 16-pass budget",
        );
        let commands = std::mem::take(&mut *self.host.commands.borrow_mut());
        println!(
            "C2E2_GUEST activation_complete replay={replaying} history_length={history_length} signal_started={} signal_completed={} commands={commands:?}",
            self.signal_started, self.signal_completed,
        );
        commands
    }
}
