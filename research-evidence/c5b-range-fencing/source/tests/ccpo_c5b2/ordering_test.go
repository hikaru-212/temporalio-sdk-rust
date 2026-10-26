package ccpo_c5b2_test

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"maps"
	"os"
	"path/filepath"
	"runtime/debug"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/google/uuid"
	"github.com/stretchr/testify/require"
	commandpb "go.temporal.io/api/command/v1"
	commonpb "go.temporal.io/api/common/v1"
	enumspb "go.temporal.io/api/enums/v1"
	historypb "go.temporal.io/api/history/v1"
	"go.temporal.io/api/serviceerror"
	taskqueuepb "go.temporal.io/api/taskqueue/v1"
	"go.temporal.io/api/workflowservice/v1"
	tokenspb "go.temporal.io/server/api/token/v1"
	"go.temporal.io/server/chasm"
	"go.temporal.io/server/common"
	"go.temporal.io/server/common/config"
	"go.temporal.io/server/common/headers"
	"go.temporal.io/server/common/log/tag"
	"go.temporal.io/server/common/metrics"
	"go.temporal.io/server/common/namespace"
	"go.temporal.io/server/common/payloads"
	"go.temporal.io/server/common/persistence"
	persistenceclient "go.temporal.io/server/common/persistence/client"
	persistencetests "go.temporal.io/server/common/persistence/persistence-tests"
	sqlstore "go.temporal.io/server/common/persistence/sql"
	"go.temporal.io/server/common/persistence/sql/sqlplugin"
	"go.temporal.io/server/common/persistence/versionhistory"
	"go.temporal.io/server/common/primitives"
	"go.temporal.io/server/common/resolver"
	"go.temporal.io/server/common/tasktoken"
	"go.temporal.io/server/common/testing/testhooks"
	"go.temporal.io/server/common/testing/testlogger"
	"go.temporal.io/server/service/history/tasks"
	"go.temporal.io/server/temporal"
	"go.temporal.io/server/tests/testcore"
	"google.golang.org/protobuf/encoding/protojson"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/types/known/durationpb"
)

const sourceSHA = "0d5e3848bed625907987f73d5eaae8dc77bc6590"

type experiment struct {
	t                           *testing.T
	ctx                         context.Context
	cluster                     *testcore.TestCluster
	client                      workflowservice.WorkflowServiceClient
	ns, nsID, workflowID, runID string
	queue                       *taskqueuepb.TaskQueue
	shardID                     int32
	token                       *tokenspb.Task
	request                     *workflowservice.RespondWorkflowTaskCompletedRequest
	requestBytes                []byte
	sequence                    atomic.Int64
	recordMu                    sync.Mutex
	preserveResources           bool
	oldRange                    atomic.Int64
	armed                       atomic.Bool
	faults                      atomic.Int32
	deleteRelease               chan struct{}
	deleteOnce                  sync.Once
	release                     func()
	gate                        chan struct{}
	gateEntered                 chan struct{}
	gateOnce                    sync.Once
	released                    atomic.Bool
	done                        chan struct{}
	started                     atomic.Bool
	delegateCancel              context.CancelFunc
	delegateErr                 error
	tx                          *observedTx
	probe                       chan chan probeResult
	recovery                    chan struct{}
	recoveryOnce                sync.Once
	renewResult                 chan error
	fresh                       chan readEvidence
	captureFresh                atomic.Bool
	managerError                chan struct{}
	connections                 *connectionAudit
	arm                         string
	launch                      chan struct{}
	launchOnce                  sync.Once
	launched                    atomic.Bool
	delegateContextErr          error
	lastLedger                  map[string]any
	dispatched                  bool
}

func (e *experiment) record(stage string, value any) {
	e.recordMu.Lock()
	defer e.recordMu.Unlock()
	if e.preserveResources {
		return
	}
	b, err := json.Marshal(map[string]any{"sequence": e.sequence.Add(1), "utc": time.Now().UTC().Format(time.RFC3339Nano), "stage": stage, "value": value})
	if err != nil {
		e.t.Errorf("evidence serialization: %v", err)
		return
	}
	e.t.Logf("CCPO %s", b)
}

func jsonProto(t *testing.T, value proto.Message) json.RawMessage {
	t.Helper()
	b, err := protojson.MarshalOptions{UseProtoNames: true}.Marshal(value)
	require.NoError(t, err)
	return b
}

func newExperiment(t *testing.T, name string) *experiment {
	t.Helper()
	ctx, cancel := context.WithTimeout(headers.SetVersions(context.Background()), 120*time.Second)
	t.Cleanup(cancel)
	e := &experiment{t: t, ctx: ctx, arm: name, launch: make(chan struct{}), ns: "ccpo-c5b2-" + name, workflowID: "ccpo-c5b2-" + name,
		deleteRelease: make(chan struct{}), gate: make(chan struct{}), gateEntered: make(chan struct{}), done: make(chan struct{}),
		probe: make(chan chan probeResult), recovery: make(chan struct{}), renewResult: make(chan error, 1), fresh: make(chan readEvidence, 8), managerError: make(chan struct{}, 1),
		queue: &taskqueuepb.TaskQueue{Name: "ccpo-c5b1-queue", Kind: enumspb.TASK_QUEUE_KIND_NORMAL}}
	logger := testlogger.NewTestLogger(t, testlogger.FailOnExpectedErrorOnly)
	logger.Expect(testlogger.Error, ".*", tag.FailedAssertion)
	db := persistencetests.GetSQLiteFileTestClusterOption()
	db.ConnectAttributes["cache"] = "private"
	db.ConnectAttributes["read_uncommitted"] = "0"
	e.connections = newConnectionAudit(e, db.DBName)
	t.Cleanup(e.connections.close)
	delegateAttributes := maps.Clone(db.ConnectAttributes)
	// A distinct DSN preserves the normal service pool while the selected write retains its own connection.
	delegateAttributes["busy_timeout"] = "1000"
	cfgSQL := &config.SQL{PluginName: "sqlite", DatabaseName: db.DBName, ConnectAttributes: delegateAttributes, MaxConns: 4, MaxIdleConns: 4}
	realDB, err := sqlstore.NewSQLDB(sqlplugin.DbKindMain, cfgSQL, resolver.NewNoopResolver(), logger, metrics.NoopMetricsHandler)
	require.NoError(t, err)
	t.Cleanup(func() {
		if !e.preserveResources {
			require.NoError(t, realDB.Close())
		}
	})
	e.record("sql-config", cfgSQL)
	observedLogger := &recoveryLogger{Logger: logger, e: e}
	baseFactory := temporal.PersistenceFactoryProvider()
	cfg := &testcore.TestClusterConfig{
		Persistence: *db, HistoryConfig: testcore.HistoryConfig{NumHistoryShards: 1},
		WorkerConfig: testcore.WorkerConfig{DisableWorker: true},
		AdditionalServerOptions: []temporal.ServerOption{temporal.WithPersistenceFactoryProvider(func(params persistenceclient.NewFactoryParams) persistenceclient.Factory {
			if params.ServiceName != primitives.HistoryService {
				return baseFactory(params)
			}
			params.DataStoreFactory = &observedStoreFactory{DataStoreFactory: params.DataStoreFactory, e: e, db: &observedDB{DB: realDB, e: e}, logger: params.Logger, serializer: params.Serializer}
			return &observedFactory{Factory: baseFactory(params), e: e}
		})},
	}
	cluster, err := testcore.NewTestClusterFactory().NewCluster(t, cfg, observedLogger)
	require.NoError(t, err)
	e.cluster = cluster
	t.Cleanup(func() {
		if e.preserveResources {
			return
		}
		e.allowDelete()
		require.NoError(t, cluster.TearDownCluster())
		logger.Close()
	})
	e.client = cluster.FrontendClient()
	e.record("baseline", map[string]any{"source_sha": sourceSHA, "database": "file SQLite", "sqlite_attributes": db.ConnectAttributes, "namespace": e.ns, "workflow_id": e.workflowID, "wft_timeout_seconds": 120, "external_activity_executor": false})
	_, err = e.client.RegisterNamespace(ctx, &workflowservice.RegisterNamespaceRequest{Namespace: e.ns, WorkflowExecutionRetentionPeriod: durationpb.New(24 * time.Hour)})
	require.NoError(t, err)
	var described *workflowservice.DescribeNamespaceResponse
	require.Eventually(t, func() bool {
		described, err = e.client.DescribeNamespace(ctx, &workflowservice.DescribeNamespaceRequest{Namespace: e.ns})
		return err == nil
	}, 10*time.Second, 20*time.Millisecond)
	e.nsID = described.NamespaceInfo.Id
	e.shardID = common.WorkflowIDToHistoryShard(e.nsID, e.workflowID, 1)
	gate := make(chan struct{})
	var once sync.Once
	e.release = func() { once.Do(func() { close(gate) }) }
	// Hold dispatch so the committed task row can be read before a queue ack removes it.
	remove := cluster.InjectHook(t, testhooks.NewHook(testhooks.HistoryTransferTaskInterceptor, func(task tasks.Task, run func()) {
		if a, ok := task.(*tasks.ActivityTask); ok && a.WorkflowID == e.workflowID {
			<-gate
		}
		run()
	}), namespace.ID(e.nsID))
	t.Cleanup(func() { e.cleanupDelegate(); e.release(); remove() })
	started, err := e.client.StartWorkflowExecution(ctx, &workflowservice.StartWorkflowExecutionRequest{Namespace: e.ns, WorkflowId: e.workflowID, RequestId: uuid.NewString(), WorkflowType: &commonpb.WorkflowType{Name: "ccpo-c5b1-workflow"}, TaskQueue: e.queue, WorkflowRunTimeout: durationpb.New(10 * time.Minute), WorkflowTaskTimeout: durationpb.New(120 * time.Second), Identity: "ccpo-client"})
	require.NoError(t, err)
	e.runID = started.RunId
	poll, err := e.client.PollWorkflowTaskQueue(ctx, &workflowservice.PollWorkflowTaskQueueRequest{Namespace: e.ns, TaskQueue: e.queue, Identity: "ccpo-client"})
	require.NoError(t, err)
	require.NotEmpty(t, poll.TaskToken)
	e.token, err = tasktoken.NewSerializer().Deserialize(poll.TaskToken)
	require.NoError(t, err)
	require.EqualValues(t, 2, e.token.ScheduledEventId)
	require.EqualValues(t, 3, e.token.StartedEventId)
	require.EqualValues(t, 1, e.token.Attempt)
	require.Empty(t, poll.NextPageToken)
	e.request = &workflowservice.RespondWorkflowTaskCompletedRequest{Namespace: e.ns, TaskToken: poll.TaskToken, Identity: "ccpo-client", Commands: []*commandpb.Command{{CommandType: enumspb.COMMAND_TYPE_SCHEDULE_ACTIVITY_TASK, Attributes: &commandpb.Command_ScheduleActivityTaskCommandAttributes{ScheduleActivityTaskCommandAttributes: &commandpb.ScheduleActivityTaskCommandAttributes{ActivityId: "ccpo-activity-1", ActivityType: &commonpb.ActivityType{Name: "ccpo-activity"}, TaskQueue: e.queue, Input: payloads.EncodeString("ccpo-input"), ScheduleToCloseTimeout: durationpb.New(5 * time.Minute), StartToCloseTimeout: durationpb.New(time.Minute), RequestEagerExecution: false}}}}}
	e.requestBytes, err = proto.MarshalOptions{Deterministic: true}.Marshal(e.request)
	require.NoError(t, err)
	e.record("request", map[string]any{"token": jsonProto(t, e.token), "token_hex": hex.EncodeToString(poll.TaskToken), "request": jsonProto(t, e.request), "sha256": fmt.Sprintf("%x", sha256.Sum256(e.requestBytes))})
	e.saveProto("request", e.request)
	e.saveProto("task-token", e.token)
	e.inspect("initial", false)
	if info, ok := debug.ReadBuildInfo(); ok {
		for _, dep := range info.Deps {
			if dep.Path == "modernc.org/sqlite" { e.record("build-version", map[string]any{"go": info.GoVersion, "driver": dep.Path, "version": dep.Version, "sum": dep.Sum}) }
		}
	}
	return e
}

func (e *experiment) complete(label string) error {
	b, err := proto.MarshalOptions{Deterministic: true}.Marshal(e.request)
	require.NoError(e.t, err)
	require.Equal(e.t, e.requestBytes, b)
	response, err := e.client.RespondWorkflowTaskCompleted(e.ctx, proto.Clone(e.request).(*workflowservice.RespondWorkflowTaskCompletedRequest))
	e.record(label, map[string]any{"error_type": fmt.Sprintf("%T", err), "error": fmt.Sprint(err), "received_success": err == nil, "response": response})
	return err
}

func (e *experiment) inspect(label string, accepted bool) *persistence.GetWorkflowExecutionResponse {
	t := e.t
	manager := e.cluster.ExecutionManager()
	state, err := manager.GetWorkflowExecution(e.ctx, &persistence.GetWorkflowExecutionRequest{ShardID: e.shardID, NamespaceID: e.nsID, WorkflowID: e.workflowID, RunID: e.runID, ArchetypeID: chasm.WorkflowArchetypeID})
	require.NoError(t, err)
	shard, err := e.cluster.TestBase().ShardMgr.GetOrCreateShard(e.ctx, &persistence.GetOrCreateShardRequest{ShardID: e.shardID})
	require.NoError(t, err)
	branch, err := versionhistory.GetCurrentVersionHistory(state.State.ExecutionInfo.VersionHistories)
	require.NoError(t, err)
	history, err := manager.ReadHistoryBranch(e.ctx, &persistence.ReadHistoryBranchRequest{ShardID: e.shardID, BranchToken: branch.BranchToken, MinEventID: 1, MaxEventID: state.State.NextEventId, PageSize: 100})
	require.NoError(t, err)
	require.Empty(t, history.NextPageToken)
	e.saveProto(label+"-mutable-state", state.State)
	e.saveProto(label+"-history", &historypb.History{Events: history.HistoryEvents})
	e.saveProto(label+"-shard", shard.ShardInfo)
	completed, scheduled := 0, 0
	for _, event := range history.HistoryEvents {
		switch event.EventType {
		case enumspb.EVENT_TYPE_WORKFLOW_TASK_COMPLETED:
			completed++
			require.EqualValues(t, 2, event.GetWorkflowTaskCompletedEventAttributes().ScheduledEventId)
			require.EqualValues(t, 3, event.GetWorkflowTaskCompletedEventAttributes().StartedEventId)
		case enumspb.EVENT_TYPE_ACTIVITY_TASK_SCHEDULED:
			scheduled++
			require.Equal(t, "ccpo-activity-1", event.GetActivityTaskScheduledEventAttributes().ActivityId)
			require.EqualValues(t, 4, event.GetActivityTaskScheduledEventAttributes().WorkflowTaskCompletedEventId)
		}
	}
	storedTasks, ledger := e.enumerate()
	e.lastLedger = ledger
	if label == "initial" {
		e.oldRange.Store(shard.ShardInfo.RangeId)
	}
	e.record(label, map[string]any{"db_revision": state.DBRecordVersion, "range_id": shard.ShardInfo.RangeId, "state": jsonProto(t, state.State), "effective_history": jsonProto(t, &historypb.History{Events: history.HistoryEvents}), "valid_completions": completed, "activity_schedules": scheduled, "direct_persisted_activity_tasks": storedTasks, "independent_category_ranges": ledger})
	if accepted {
		require.Equal(t, 1, completed)
		require.Equal(t, 1, scheduled)
		if !e.dispatched { require.EqualValues(t, 4, state.DBRecordVersion); require.EqualValues(t, 6, state.State.NextEventId) } else { require.GreaterOrEqual(t, state.DBRecordVersion, int64(4)); require.GreaterOrEqual(t, state.State.NextEventId, int64(6)) }
		require.Zero(t, state.State.ExecutionInfo.WorkflowTaskScheduledEventId)
		require.Zero(t, state.State.ExecutionInfo.WorkflowTaskStartedEventId)
		require.Len(t, state.State.ActivityInfos, 1)
		require.Equal(t, "ccpo-activity-1", state.State.ActivityInfos[5].ActivityId)
		if !e.dispatched { require.Len(t, storedTasks, 1) } else { require.LessOrEqual(t, len(storedTasks), 1) }
		for _,task := range storedTasks { require.EqualValues(t, 5, task.(*tasks.ActivityTask).ScheduledEventID) }
	} else {
		require.EqualValues(t, 3, state.DBRecordVersion)
		require.Zero(t, completed)
		require.Zero(t, scheduled)
		require.EqualValues(t, 4, state.State.NextEventId)
		require.EqualValues(t, 2, state.State.ExecutionInfo.WorkflowTaskScheduledEventId)
		require.EqualValues(t, 3, state.State.ExecutionInfo.WorkflowTaskStartedEventId)
		require.EqualValues(t, 1, state.State.ExecutionInfo.WorkflowTaskAttempt)
		require.Empty(t, state.State.ActivityInfos)
		require.Empty(t, storedTasks)
	}
	return state
}

func (e *experiment) enumerate() ([]tasks.Task, map[string]any) {
	categories := []tasks.Category{tasks.CategoryTransfer, tasks.CategoryTimer, tasks.CategoryVisibility, tasks.CategoryReplication, tasks.CategoryArchival, tasks.CategoryOutbound}
	ledger := make(map[string]any)
	var activities []tasks.Task
	for _, category := range categories {
		request := &persistence.GetHistoryTasksRequest{ShardID: e.shardID, TaskCategory: category, InclusiveMinTaskKey: tasks.MinimumKey, ExclusiveMaxTaskKey: tasks.MaximumKey, BatchSize: 2}
		if category.Type() == tasks.CategoryTypeImmediate {
			request.ExclusiveMaxTaskKey = tasks.NewImmediateKey(tasks.MaximumKey.TaskID)
		} else {
			request.ExclusiveMaxTaskKey = tasks.NewKey(tasks.MaximumKey.FireTime, 0)
		}
		var matching []tasks.Task
		pages, rows := 0, 0
		for {
			page, err := e.cluster.ExecutionManager().GetHistoryTasks(e.ctx, request)
			require.NoError(e.t, err)
			pages++
			rows += len(page.Tasks)
			e.record("task-range-page",map[string]any{"category":category.Name(),"page":pages,"rows":len(page.Tasks),"request_token":request.NextPageToken,"next_token":page.NextPageToken,"tasks":page.Tasks,"inclusive_min":request.InclusiveMinTaskKey,"exclusive_max":request.ExclusiveMaxTaskKey})
			for _, task := range page.Tasks {
				if task.GetNamespaceID() == e.nsID && task.GetWorkflowID() == e.workflowID && task.GetRunID() == e.runID {
					matching = append(matching, task)
					if _, ok := task.(*tasks.ActivityTask); ok {
						activities = append(activities, task)
					}
				}
			}
			if len(page.NextPageToken) == 0 {
				break
			}
			require.Less(e.t, pages, 100, "unexpected task range pagination")
			request.NextPageToken = page.NextPageToken
		}
		ledger[category.Name()] = map[string]any{"inclusive_min": request.InclusiveMinTaskKey, "exclusive_max": request.ExclusiveMaxTaskKey, "pages": pages, "rows": rows, "matching_workflow_tasks": matching, "final_page_token_empty": true}
	}
	return activities, ledger
}

func (e *experiment) durableRange() int64 {
	row, err := e.cluster.TestBase().ShardMgr.GetOrCreateShard(e.ctx, &persistence.GetOrCreateShardRequest{ShardID: e.shardID})
	require.NoError(e.t, err)
	return row.ShardInfo.RangeId
}

func (e *experiment) saveProto(name string, value proto.Message) {
	dir := os.Getenv("CCPO_EVIDENCE_OUTPUT")
	if dir == "" {
		return
	}
	data, err := proto.MarshalOptions{Deterministic: true}.Marshal(value)
	require.NoError(e.t, err)
	f, err := os.OpenFile(filepath.Join(dir, name+".pb"), os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0600)
	require.NoError(e.t, err)
	_, err = f.Write(data)
	require.NoError(e.t, err)
	require.NoError(e.t, f.Close())
	e.record("protobuf-artifact", map[string]any{"file": name + ".pb", "type": string(value.ProtoReflect().Descriptor().FullName()), "sha256": fmt.Sprintf("%x", sha256.Sum256(data)), "bytes": len(data)})
}

func (e *experiment) awaitRecoveryAndFresh(accepted bool) {
	t:=e.t
	var timeout *serviceerror.DeadlineExceeded
	require.ErrorAs(t,e.complete("first-completion-RPC"),&timeout)
	select {case <-e.managerError:case <-e.ctx.Done():t.Fatal("INCONCLUSIVE: manager TimeoutError not observed")}
	select {case <-e.recovery:case <-e.ctx.Done():t.Fatal("INCONCLUSIVE: real uncertain-write handling absent")}
	select {case err:=<-e.renewResult:require.NoError(t,err);case <-e.ctx.Done():t.Fatal("INCONCLUSIVE: renewal absent")}
	require.EqualValues(t,2,e.durableRange())
	e.record("independent-durable-range-read",map[string]any{"range":2,"old_gate_released":e.released.Load(),"arm":e.arm})
	e.captureFresh.Store(true)
	describe,err:=e.client.DescribeWorkflowExecution(e.ctx,&workflowservice.DescribeWorkflowExecutionRequest{Namespace:e.ns,Execution:&commonpb.WorkflowExecution{WorkflowId:e.workflowID,RunId:e.runID}})
	require.NoError(t,err)
	e.saveProto("guarded-describe",describe)
	var fresh readEvidence
	select {case fresh=<-e.fresh:case <-e.ctx.Done():t.Fatal("INCONCLUSIVE: fresh guarded reload absent")}
	e.captureFresh.Store(false)
	require.NoError(t,fresh.Err)
	require.Contains(t,fresh.Stack,"shard.(*ContextImpl).GetWorkflowExecution")
	require.Contains(t,fresh.Stack,"workflow.(*ContextImpl).LoadMutableState")
	require.True(t,strings.Contains(fresh.Stack,"describeworkflow"),fresh.Stack)
	if accepted {
		require.EqualValues(t,4,fresh.Revision);require.EqualValues(t,6,fresh.Next);require.Zero(t,fresh.Scheduled);require.Zero(t,fresh.Started)
	} else {
		require.EqualValues(t,3,fresh.Revision);require.EqualValues(t,4,fresh.Next);require.EqualValues(t,2,fresh.Scheduled);require.EqualValues(t,3,fresh.Started);require.False(t,fresh.Released)
	}
}

func (e *experiment) probeOld() {
	t:=e.t
	require.False(t,e.released.Load());require.False(t,e.tx.closed.Load());require.Zero(t,e.tx.mutations.Load())
	response:=make(chan probeResult,1)
	select {case e.probe<-response:case <-e.ctx.Done():t.Fatal("INCONCLUSIVE: old probe not admitted")}
	select {case p:=<-response:require.NoError(t,p.Err);require.EqualValues(t,1,p.Range);require.Equal(t,e.tx.id,p.TxID);case <-e.ctx.Done():t.Fatal("INCONCLUSIVE: old transaction probe failed")}
	e.connections.mu.Lock();count:=len(e.connections.ids);e.connections.mu.Unlock()
	require.GreaterOrEqual(t,count,2)
	e.record("old-transaction-confirmed-across-barrier",map[string]any{"tx":e.tx.id,"connections":count,"snapshot_range":1,"released":false,"mutations":e.tx.mutations.Load()})
}

func (e *experiment) pollActivity() {
	t:=e.t
	e.dispatched=true
	e.allowDelete();e.release()
	ctx,cancel:=context.WithTimeout(e.ctx,15*time.Second);defer cancel()
	poll,err:=e.client.PollActivityTaskQueue(ctx,&workflowservice.PollActivityTaskQueueRequest{Namespace:e.ns,TaskQueue:e.queue,Identity:"ccpo-recovery-observer"})
	e.record("normal-Activity-poll",map[string]any{"error":fmt.Sprint(err),"response":poll,"external_effect_executed":false})
	require.NoError(t,err);require.NotEmpty(t,poll.TaskToken)
	require.Equal(t,"ccpo-activity-1",poll.ActivityId);require.Equal(t,e.runID,poll.WorkflowExecution.RunId)
	require.True(t,proto.Equal(payloads.EncodeString("ccpo-input"),poll.Input))
	token,err:=tasktoken.NewSerializer().Deserialize(poll.TaskToken);require.NoError(t,err)
	require.EqualValues(t,5,token.ScheduledEventId)
	e.saveProto("activity-poll",poll);e.saveProto("activity-token",token)
	e.inspect("after-Activity-poll",true)
}

func TestCCPOC5B2Main(t *testing.T) {
	e:=newExperiment(t,"main")
	before:=e.inspect("before-fault",false)
	require.EqualValues(t,1,e.durableRange())
	e.armed.Store(true)
	e.awaitRecoveryAndFresh(false)
	e.probeOld()
	pre:=e.inspect("before-old-release",false);preTasks:=e.lastLedger
	require.Equal(t,before.DBRecordVersion,pre.DBRecordVersion);require.True(t,proto.Equal(before.State,pre.State))
	e.record("G-release-old-transaction",map[string]any{"tx":e.tx.id,"range":e.durableRange()})
	require.True(t, e.cleanupDelegate(), "INCONCLUSIVE: delegate termination not confirmed")
	require.NoError(t,e.delegateContextErr,"INCONCLUSIVE: delegate canceled before physical outcome")
	require.True(t,e.tx.closed.Load())
	post:=e.inspect("J-after-old-termination",false);postTasks:=e.lastLedger
	require.EqualValues(t,2,e.durableRange())
	require.Equal(t,pre.DBRecordVersion,post.DBRecordVersion);require.True(t,proto.Equal(pre.State,post.State));require.Equal(t,preTasks,postTasks)
	require.Error(t,e.delegateErr);require.EqualValues(t,517,e.tx.sqlCode.Load());require.Zero(t,e.tx.commits.Load());require.EqualValues(t,1,e.tx.rollbacks.Load());require.EqualValues(t,1,e.tx.mutations.Load())
	e.record("post-barrier-state-preserved",map[string]any{"revision":post.DBRecordVersion,"same_MS":true,"same_six_category_tasks":true,"range":2,"sql_code":e.tx.sqlCode.Load()})
	require.NoError(t,e.complete("K-legitimate-identical-retry"))
	final:=e.inspect("L-after-legitimate-retry",true)
	require.EqualValues(t,4,final.DBRecordVersion);require.EqualValues(t,2,e.durableRange())
	e.pollActivity()
	e.record("arm-safety-confirmed",map[string]any{"arm":"main","scope":"controlled SQLite normal WFT; controls still required","valid_acceptances":1,"external_effects":0})
}

func TestCCPOC5B2Positive(t *testing.T) {
	e:=newExperiment(t,"positive")
	require.EqualValues(t,1,e.durableRange());e.armed.Store(true)
	e.awaitRecoveryAndFresh(true)
	select {case <-e.done:default:t.Fatal("INCONCLUSIVE: positive delegate did not terminate")}
	require.NoError(t,e.delegateErr);require.NoError(t,e.delegateContextErr)
	require.EqualValues(t,1,e.tx.rangeRead.Load());require.EqualValues(t,1,e.tx.commits.Load());require.Zero(t,e.tx.rollbacks.Load());require.EqualValues(t,1,e.tx.mutations.Load())
	state:=e.inspect("positive-after-renewal",true);ledger:=e.lastLedger
	var nf *serviceerror.NotFound
	require.ErrorAs(t,e.complete("positive-identical-retry"),&nf)
	after:=e.inspect("positive-after-rejected-retry",true)
	require.Equal(t,state.DBRecordVersion,after.DBRecordVersion);require.True(t,proto.Equal(state.State,after.State));require.Equal(t,ledger,e.lastLedger);require.EqualValues(t,2,e.durableRange())
	e.pollActivity()
	e.record("arm-safety-confirmed",map[string]any{"arm":"positive","valid_acceptances":1,"external_effects":0})
}

func TestCCPOC5B2Negative(t *testing.T) {
	e:=newExperiment(t,"negative")
	require.EqualValues(t,1,e.durableRange());e.armed.Store(true)
	e.awaitRecoveryAndFresh(false)
	pre:=e.inspect("negative-before-old-launch",false);ledger:=e.lastLedger
	require.False(t,e.launched.Load())
	e.record("negative-launch-after-authoritative-read",map[string]any{"range":e.durableRange(),"request_range":1,"revision":pre.DBRecordVersion})
	e.launchOnce.Do(func(){e.launched.Store(true);close(e.launch)})
	select {case <-e.done:case <-e.ctx.Done():t.Fatal("INCONCLUSIVE: negative delegate not terminated")}
	require.NoError(t,e.delegateContextErr)
	post:=e.inspect("negative-after-stale-attempt",false)
	require.Equal(t,pre.DBRecordVersion,post.DBRecordVersion);require.True(t,proto.Equal(pre.State,post.State));require.Equal(t,ledger,e.lastLedger)
	var lost *persistence.ShardOwnershipLostError
	require.ErrorAs(t,e.delegateErr,&lost)
	require.NotNil(t,e.tx);require.EqualValues(t,2,e.tx.rangeRead.Load());require.Zero(t,e.tx.gateVisits.Load());require.Zero(t,e.tx.mutations.Load());require.Zero(t,e.tx.commits.Load());require.EqualValues(t,1,e.tx.rollbacks.Load());require.True(t,e.tx.closed.Load())
	require.EqualValues(t,2,e.durableRange())
	require.NoError(t,e.complete("negative-legitimate-retry"))
	e.inspect("negative-after-legitimate-retry",true)
	e.pollActivity()
	e.record("arm-safety-confirmed",map[string]any{"arm":"negative","valid_acceptances":1,"external_effects":0})
}
