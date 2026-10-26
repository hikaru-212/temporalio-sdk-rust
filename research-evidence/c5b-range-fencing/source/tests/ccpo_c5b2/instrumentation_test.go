package ccpo_c5b2_test

import (
	"context"
	"database/sql"
	"database/sql/driver"
	"errors"
	"fmt"
	"runtime/debug"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	enumspb "go.temporal.io/api/enums/v1"
	"go.temporal.io/server/common/log"
	"go.temporal.io/server/common/log/tag"
	"go.temporal.io/server/common/persistence"
	persistenceclient "go.temporal.io/server/common/persistence/client"
	"go.temporal.io/server/common/persistence/serialization"
	sqlstore "go.temporal.io/server/common/persistence/sql"
	"go.temporal.io/server/common/persistence/sql/sqlplugin"
	modernc "modernc.org/sqlite"
)

type delegateKey struct{}
type probeResult struct {
	Range int64
	Err   error
	TxID  string
}
type readEvidence struct {
	Revision                 int64
	Next, Scheduled, Started int64
	Stack                    string
	Err                      error
	Released                 bool
}

type observedStoreFactory struct {
	persistence.DataStoreFactory
	e          *experiment
	db         sqlplugin.DB
	logger     log.Logger
	serializer serialization.Serializer
}

func (f *observedStoreFactory) NewExecutionStore() (persistence.ExecutionStore, error) {
	regular, err := f.DataStoreFactory.NewExecutionStore()
	if err != nil {
		return nil, err
	}
	s, err := sqlstore.NewSQLExecutionStore(f.db, f.logger, f.serializer)
	if err != nil {
		return nil, err
	}
	return &asyncStore{ExecutionStore: regular, delegate: s, serializer:f.serializer, e: f.e}, nil
}

type asyncStore struct {
	persistence.ExecutionStore
	delegate persistence.ExecutionStore
	serializer serialization.Serializer
	e        *experiment
}

func (s *asyncStore) UpdateWorkflowExecution(ctx context.Context, r *persistence.InternalUpdateWorkflowExecutionRequest) error {
	e := s.e
	if r == nil || r.UpdateWorkflowMutation.ExecutionInfo == nil { return fmt.Errorf("INCONCLUSIVE: nil internal request metadata") }
	if r.UpdateWorkflowMutation.ExecutionInfo.WorkflowId != e.workflowID || !e.armed.CompareAndSwap(true, false) {
		return s.ExecutionStore.UpdateWorkflowExecution(ctx, r)
	}
	if err := s.validateRequest(r); err != nil { return err }
	owned, err := cloneRequest(r)
	if err != nil {
		return err
	}
	audit, err := verifyOwnership(r, owned)
	if err != nil {
		return err
	}
	e.record("ownership-audit", audit)
	e.record("internal-request", normalizedValue(owned))
	delegateContext, cancel := context.WithTimeout(context.WithValue(context.Background(), delegateKey{}, e), 90*time.Second)
	e.delegateCancel = cancel
	e.started.Store(true)
	e.faults.Add(1)
	go func() {
		defer close(e.done)
		defer func() {
			cancel()
			e.record("delegate-context-cancelled", map[string]any{"context_error": fmt.Sprint(delegateContext.Err())})
		}()
		if e.arm == "negative" {
			select { case <-e.launch: case <-delegateContext.Done(): }
		}
		if delegateContext.Err() != nil { e.delegateErr = delegateContext.Err() } else {
			e.record("delegate-invocation",map[string]any{"arm":e.arm,"range":owned.RangeID,"launched":e.launched.Load()})
			e.delegateErr = s.delegate.UpdateWorkflowExecution(delegateContext, owned)
		}
		e.delegateContextErr = delegateContext.Err()
		e.record("delegate-terminated", map[string]any{"error": fmt.Sprint(e.delegateErr), "type": fmt.Sprintf("%T", e.delegateErr), "context_before_cancel": fmt.Sprint(delegateContext.Err())})
	}()
	if e.arm == "negative" {
		e.record("injected-store-timeout-before-delegate",map[string]any{"delegate_started":false,"preserved_request_range":owned.RangeID})
		return &persistence.TimeoutError{Msg:"CCPO C5B2 controlled unknown outcome before old delegate launch"}
	}
	select {
	case <-e.gateEntered:
		if e.arm == "positive" {
			e.releaseOld()
			select { case <-e.done: case <-ctx.Done(): return ctx.Err() }
			if e.delegateErr != nil { return fmt.Errorf("INCONCLUSIVE: positive old transaction did not commit: %w",e.delegateErr) }
			e.record("positive-committed-before-error",map[string]any{"commits":e.tx.commits.Load(),"tx":e.tx.id})
		}
		e.record("injected-store-timeout", map[string]any{"delegate_still_open": !e.tx.closed.Load(), "layer": "ExecutionStore.UpdateWorkflowExecution", "native_database_timeout": false})
		return &persistence.TimeoutError{Msg: "CCPO C5B2 controlled ambiguous persistence result"}
	case <-e.done:
		return fmt.Errorf("INCONCLUSIVE: delegate exited before gate: %w", e.delegateErr)
	case <-ctx.Done():
		return ctx.Err()
	}
}

func (s *asyncStore) validateRequest(r *persistence.InternalUpdateWorkflowExecutionRequest) error {
	m:=&r.UpdateWorkflowMutation
	if r.NewWorkflowSnapshot != nil || len(r.NewWorkflowNewEvents)!=0 { return fmt.Errorf("INCONCLUSIVE: unsupported new workflow request") }
	if r.RangeID!=1 || r.Mode!=persistence.UpdateWorkflowModeUpdateCurrent || m.ExecutionState==nil || m.NextEventID!=6 || m.DBRecordVersion!=4 || m.Condition!=4 || len(m.UpsertActivityInfos)!=1 || m.UpsertActivityInfos[5]==nil || len(r.UpdateWorkflowNewEvents)!=1 {
		return fmt.Errorf("INCONCLUSIVE: request is outside registered normal WFT shape")
	}
	if len(m.DeleteActivityInfos)+len(m.UpsertTimerInfos)+len(m.DeleteTimerInfos)+len(m.UpsertChildExecutionInfos)+len(m.DeleteChildExecutionInfos)+len(m.UpsertRequestCancelInfos)+len(m.DeleteRequestCancelInfos)+len(m.UpsertSignalInfos)+len(m.DeleteSignalInfos)+len(m.UpsertChasmNodes)+len(m.DeleteChasmNodes)+len(m.UpsertSignalRequestedIDs)+len(m.DeleteSignalRequestedIDs)!=0 || m.NewBufferedEvents!=nil || m.ClearBufferedEvents {
		return fmt.Errorf("INCONCLUSIVE: unrelated mutation fields")
	}
	batch:=r.UpdateWorkflowNewEvents[0]
	if batch==nil || batch.IsNewBranch || batch.BranchInfo==nil || len(batch.BranchToken)==0 || batch.Node.NodeID!=4 { return fmt.Errorf("INCONCLUSIVE: unsupported history batch") }
	events,err:=s.serializer.DeserializeEvents(batch.Node.Events)
	if err!=nil { return fmt.Errorf("INCONCLUSIVE: history decode: %w",err) }
	if len(events)!=2 || events[0].EventId!=4 || events[0].EventType!=enumspb.EVENT_TYPE_WORKFLOW_TASK_COMPLETED || events[1].EventId!=5 || events[1].EventType!=enumspb.EVENT_TYPE_ACTIVITY_TASK_SCHEDULED { return fmt.Errorf("INCONCLUSIVE: unexpected completion history") }
	w,a:=events[0].GetWorkflowTaskCompletedEventAttributes(),events[1].GetActivityTaskScheduledEventAttributes()
	if w==nil || a==nil || w.ScheduledEventId!=2 || w.StartedEventId!=3 || a.WorkflowTaskCompletedEventId!=4 || a.ActivityId!="ccpo-activity-1" { return fmt.Errorf("INCONCLUSIVE: invalid causal references") }
	s.e.record("normal-request-shape-verified",map[string]any{"range":r.RangeID,"revision":m.DBRecordVersion,"condition":m.Condition,"next":m.NextEventID,"history_ids":[]int64{4,5},"new_snapshot":false,"new_workflow_events":0})
	return nil
}

type observedDB struct {
	sqlplugin.DB
	e *experiment
}

func (db *observedDB) BeginTx(ctx context.Context) (sqlplugin.Tx, error) {
	tx, err := db.DB.BeginTx(ctx)
	if err != nil || ctx.Value(delegateKey{}) != db.e {
		return tx, err
	}
	w := &observedTx{Tx: tx, e: db.e, id: fmt.Sprintf("%T:%p", tx, tx)}
	db.e.tx = w
	db.e.record("A-real-BeginTx", map[string]any{"tx": w.id, "error": fmt.Sprint(err)})
	return w, nil
}

func (db *observedDB) InsertIntoHistoryNode(ctx context.Context, r *sqlplugin.HistoryNodeRow) (sql.Result, error) {
	result, err := db.DB.InsertIntoHistoryNode(ctx, r)
	if ctx.Value(delegateKey{}) == db.e {
		db.e.record("history-append-before-MS-transaction", map[string]any{"row": r, "error": fmt.Sprint(err), "acceptance_receipt": false})
	}
	return result, err
}

type observedTx struct {
	sqlplugin.Tx
	e         *experiment
	id        string
	closed    atomic.Bool
	mutations atomic.Int32
	commits atomic.Int32
	rollbacks atomic.Int32
	rangeRead atomic.Int64
	sqlCode atomic.Int32
	gateVisits atomic.Int32
}

func (tx *observedTx) ReadLockShards(ctx context.Context, f sqlplugin.ShardsFilter) (int64, error) {
	r, err := tx.Tx.ReadLockShards(ctx, f)
	if err==nil { tx.rangeRead.Store(r) }
	tx.e.record("B-real-shard-read", map[string]any{"tx": tx.id, "range": r, "error": fmt.Sprint(err)})
	return r, err
}

func (tx *observedTx) LockCurrentExecutions(ctx context.Context, f sqlplugin.CurrentExecutionsFilter) (*sqlplugin.CurrentExecutionsRow, error) {
	row, err := tx.Tx.LockCurrentExecutions(ctx, f)
	if err != nil {
		return row, err
	}
	e := tx.e
	tx.gateVisits.Add(1)
	e.record("C-pre-mutation-gate", map[string]any{"tx": tx.id, "real_current_row": row, "mutations": tx.mutations.Load(), "stack": string(debug.Stack())})
	close(e.gateEntered)
	for {
		select {
		case response := <-e.probe:
			r, readErr := tx.Tx.ReadLockShards(ctx, sqlplugin.ShardsFilter{ShardID: e.shardID})
			e.record("open-old-transaction-probe", map[string]any{"tx": tx.id, "range": r, "error": fmt.Sprint(readErr), "context_error": fmt.Sprint(ctx.Err()), "released": e.released.Load()})
			response <- probeResult{Range: r, Err: readErr, TxID: tx.id}
		case <-e.gate:
			e.record("old-gate-release", map[string]any{"tx": tx.id, "context_error": fmt.Sprint(ctx.Err())})
			return row, nil
		case <-ctx.Done():
			return nil, ctx.Err()
		}
	}
}

func (tx *observedTx) UpdateCurrentExecutions(ctx context.Context, row *sqlplugin.CurrentExecutionsRow) (sql.Result, error) {
	tx.mutations.Add(1)
	tx.e.record("first-SQL-mutation", map[string]any{"tx": tx.id, "method": "UpdateCurrentExecutions", "released": tx.e.released.Load(),"context_error":fmt.Sprint(ctx.Err())})
	r, err := tx.Tx.UpdateCurrentExecutions(ctx, row)
	var se *modernc.Error
	code := 0
	if errors.As(err, &se) {
		code = se.Code()
	}
	tx.sqlCode.Store(int32(code))
	tx.e.record("raw-SQL-result", map[string]any{"tx": tx.id, "error": fmt.Sprint(err), "type": fmt.Sprintf("%T", err), "sqlite_code": code})
	return r, err
}

func (tx *observedTx) Commit() error {
	err := tx.Tx.Commit()
	if err==nil { tx.commits.Add(1) }
	tx.closed.Store(err == nil || errors.Is(err, sql.ErrTxDone))
	tx.e.record("real-tx-commit-return", map[string]any{"tx": tx.id, "error": fmt.Sprint(err)})
	return err
}

func (tx *observedTx) Rollback() error {
	err := tx.Tx.Rollback()
	if err==nil { tx.rollbacks.Add(1) }
	tx.closed.Store(err == nil || errors.Is(err, sql.ErrTxDone))
	tx.e.record("real-tx-rollback-return", map[string]any{"tx": tx.id, "error": fmt.Sprint(err)})
	return err
}

type recoveryLogger struct {
	log.Logger
	e *experiment
}

func (l *recoveryLogger) Info(msg string, tags ...tag.Tag) {
	if l.e.faults.Load() > 0 && msg == "" {
		stack := string(debug.Stack())
		if strings.Contains(stack, "handleWriteErrorLocked") && strings.Contains(stack, ".transition") {
			l.e.recoveryOnce.Do(func() {
				l.e.record("F-real-uncertain-write-transition", map[string]any{"stack": stack, "state": "Acquiring; set before this log", "E_tracker_evidence": "source UpdateWorkflowExecution calls requestCompletionFn before handleWriteError; renewal later passes drain"})
				close(l.e.recovery)
			})
		}
	}
	l.Logger.Info(msg, tags...)
}

type observedFactory struct {
	persistenceclient.Factory
	e *experiment
}
type observedExecution struct {
	persistence.ExecutionManager
	e *experiment
}
type observedShard struct {
	persistence.ShardManager
	e *experiment
}

func (f *observedFactory) NewExecutionManager() (persistence.ExecutionManager, error) {
	m, err := f.Factory.NewExecutionManager()
	if err != nil {
		return nil, err
	}
	return &observedExecution{ExecutionManager: m, e: f.e}, nil
}
func (f *observedFactory) NewShardManager() (persistence.ShardManager, error) {
	m, err := f.Factory.NewShardManager()
	if err != nil {
		return nil, err
	}
	return &observedShard{ShardManager: m, e: f.e}, nil
}

func (m *observedExecution) UpdateWorkflowExecution(ctx context.Context, r *persistence.UpdateWorkflowExecutionRequest) (*persistence.UpdateWorkflowExecutionResponse, error) {
	result, err := m.ExecutionManager.UpdateWorkflowExecution(ctx, r)
	if r.UpdateWorkflowMutation.ExecutionInfo.WorkflowId == m.e.workflowID && m.e.faults.Load() > 0 {
		m.e.record("D-manager-result", map[string]any{"type": fmt.Sprintf("%T", err), "error": fmt.Sprint(err), "stack": string(debug.Stack())})
		var timeout *persistence.TimeoutError
		if errors.As(err, &timeout) {
			select {
			case m.e.managerError <- struct{}{}:
			default:
			}
		}
	}
	return result, err
}

func (m *observedExecution) GetWorkflowExecution(ctx context.Context, r *persistence.GetWorkflowExecutionRequest) (*persistence.GetWorkflowExecutionResponse, error) {
	result, err := m.ExecutionManager.GetWorkflowExecution(ctx, r)
	if r.WorkflowID == m.e.workflowID && m.e.captureFresh.Load() {
		v := readEvidence{Stack: string(debug.Stack()), Err: err, Released: m.e.released.Load()}
		if result != nil {
			v.Revision = result.DBRecordVersion
			v.Next = result.State.NextEventId
			v.Scheduled = result.State.ExecutionInfo.WorkflowTaskScheduledEventId
			v.Started = result.State.ExecutionInfo.WorkflowTaskStartedEventId
		}
		m.e.record("I-guarded-fresh-MS-read", map[string]any{"revision": v.Revision, "next_event": v.Next, "scheduled": v.Scheduled, "started": v.Started, "released": v.Released, "error": fmt.Sprint(err), "stack": v.Stack, "H_evidence": "successful ContextImpl.GetWorkflowExecution passed errorByState"})
		select {
		case m.e.fresh <- v:
		default:
		}
	}
	return result, err
}

func (m *observedShard) UpdateShard(ctx context.Context, r *persistence.UpdateShardRequest) error {
	if m.e.faults.Load() == 0 || r.PreviousRangeID != 1 || r.ShardInfo.RangeId != 2 {
		return m.ShardManager.UpdateShard(ctx, r)
	}
	m.e.record("renewal-after-tracker-drain", map[string]any{"previous": r.PreviousRangeID, "requested": r.ShardInfo.RangeId, "stack": string(debug.Stack())})
	err := m.ShardManager.UpdateShard(ctx, r)
	m.e.record("G-real-range-renewal-return", map[string]any{"previous": r.PreviousRangeID, "new": r.ShardInfo.RangeId, "error": fmt.Sprint(err)})
	select {
	case m.e.renewResult <- err:
	default:
	}
	return err
}

func (e *experiment) allowDelete() { e.deleteOnce.Do(func() { close(e.deleteRelease) }) }
func (m *observedExecution) CompleteHistoryTask(ctx context.Context, r *persistence.CompleteHistoryTaskRequest) error {
	select {
	case <-m.e.deleteRelease:
		return m.ExecutionManager.CompleteHistoryTask(ctx, r)
	case <-ctx.Done():
		return ctx.Err()
	}
}
func (m *observedExecution) RangeCompleteHistoryTasks(ctx context.Context, r *persistence.RangeCompleteHistoryTasksRequest) error {
	select {
	case <-m.e.deleteRelease:
		return m.ExecutionManager.RangeCompleteHistoryTasks(ctx, r)
	case <-ctx.Done():
		return ctx.Err()
	}
}

func (e *experiment) releaseOld() {
	e.gateOnce.Do(func() { e.released.Store(true); close(e.gate) })
}
// Abandoned delegates must not log through testing.T after the test has returned.
func (e *experiment) retainDelegateResources() {
	e.recordMu.Lock()
	defer e.recordMu.Unlock()
	e.preserveResources = true
}

// A true result means no delegate was started or done was received.
func (e *experiment) cleanupDelegate() bool {
	e.releaseOld()
	if !e.started.Load() {
		return true
	}
	if e.arm=="negative" && !e.launched.Load() { e.delegateCancel(); e.launchOnce.Do(func(){close(e.launch)}) }
	timer := time.NewTimer(10 * time.Second)
	defer timer.Stop()
	select {
	case <-e.done:
	case <-timer.C:
		e.delegateCancel()
		timer.Reset(5 * time.Second)
		select {
		case <-e.done:
		case <-timer.C:
			e.retainDelegateResources()
			e.t.Error("INCONCLUSIVE: delegate did not terminate after bounded cancellation; resources retained until process exit")
			return false
		}
	}
	e.delegateCancel()
	if e.tx != nil && !e.tx.closed.Load() {
		e.retainDelegateResources()
		e.t.Error("INCONCLUSIVE: transaction closure not confirmed before cluster teardown")
	}
	e.record("cleanup-joined", map[string]any{"delegate_done": true, "cancel_called": true, "error": fmt.Sprint(e.delegateErr)})
	return true
}

type connectionAudit struct {
 mu sync.Mutex
 e *experiment
 connections []driver.Conn
 ids map[string]bool
 prefix string
 active bool
}

var hookRegistry = struct {
 sync.Mutex
 once sync.Once
 entries map[string]*connectionAudit
}{entries:make(map[string]*connectionAudit)}

func newConnectionAudit(e *experiment,dbName string) *connectionAudit {
 a:=&connectionAudit{e:e,ids:map[string]bool{},prefix:"file:"+dbName+"?",active:true}
 hookRegistry.once.Do(func(){
  modernc.RegisterConnectionHook(func(c modernc.ExecQuerierContext,dsn string) error {
   hookRegistry.Lock()
   var selected *connectionAudit
   for prefix,v:=range hookRegistry.entries { if strings.HasPrefix(dsn,prefix) { selected=v;break } }
   hookRegistry.Unlock()
   if selected==nil { return nil }
   return selected.observe(c,dsn)
  })
 })
 hookRegistry.Lock();hookRegistry.entries[a.prefix]=a;hookRegistry.Unlock()
 e.record("connection-hook-registered",map[string]any{"prefix":a.prefix,"global_dispatcher":"once per process; no testing.T capture","removable_registration":true})
 return a
}

func (a *connectionAudit) observe(c modernc.ExecQuerierContext,dsn string) error {
 a.mu.Lock();defer a.mu.Unlock()
 if !a.active { return nil }
 conn,ok:=c.(driver.Conn);if !ok {return fmt.Errorf("driver connection cannot be owned at cleanup")}
 id:=fmt.Sprintf("%T:%p",c,c)
 ctx,cancel:=context.WithTimeout(context.Background(),5*time.Second);defer cancel()
 values:=map[string]any{}
 for _,q:=range []string{"PRAGMA journal_mode","PRAGMA read_uncommitted","PRAGMA database_list"} {
  rows,err:=c.QueryContext(ctx,q,nil);if err!=nil{return err}
  v:=make([]driver.Value,len(rows.Columns()));err=rows.Next(v);closeErr:=rows.Close()
  if err!=nil{return err};if closeErr!=nil{return closeErr};values[q]=v
 }
 if fmt.Sprint(values["PRAGMA journal_mode"])!="[wal]" || fmt.Sprint(values["PRAGMA read_uncommitted"])!="[0]" || !strings.Contains(dsn,"cache=private") {return fmt.Errorf("unexpected SQLite configuration: %v %s",values,dsn)}
 a.connections=append(a.connections,conn);a.ids[id]=true
 a.e.record("real-driver-connection",map[string]any{"connection":id,"dsn":dsn,"pragma":values})
 return nil
}

// The pinned pool retains idle connections; unregister and close only after delegate and service teardown.
func (a *connectionAudit) close() {
 hookRegistry.Lock();delete(hookRegistry.entries,a.prefix);left:=len(hookRegistry.entries);hookRegistry.Unlock()
 a.mu.Lock();defer a.mu.Unlock()
 a.active=false
 if a.e.preserveResources {
  a.e.t.Logf("INCONCLUSIVE: connection hook unregistered; %d physical connections retained until process exit; registry entries=%d",len(a.connections),left)
  return
 }
 for _,c:=range a.connections {
  err:=c.Close()
  a.e.record("physical-driver-close",map[string]any{"connection":fmt.Sprintf("%T:%p",c,c),"error":fmt.Sprint(err)})
  if err!=nil {a.e.t.Errorf("INCONCLUSIVE: driver connection close: %v",err)}
 }
 a.e.record("connection-hook-unregistered",map[string]any{"remaining_registry_entries":left,"callback_no_longer_retains_test":true})
 a.connections=nil
}
