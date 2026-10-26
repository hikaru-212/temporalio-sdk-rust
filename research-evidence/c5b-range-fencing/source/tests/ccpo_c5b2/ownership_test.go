package ccpo_c5b2_test

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"reflect"
	"slices"
	"time"

	commonpb "go.temporal.io/api/common/v1"
	"go.temporal.io/server/common/persistence"
	"go.temporal.io/server/service/history/tasks"
	"google.golang.org/protobuf/proto"
)

func cloneProto[T proto.Message](p T) T {
	if reflect.ValueOf(p).IsNil() {
		var zero T
		return zero
	}
	return proto.Clone(p).(T)
}

func cloneBlobs[K comparable](m map[K]*commonpb.DataBlob) map[K]*commonpb.DataBlob {
	if m == nil {
		return nil
	}
	r := make(map[K]*commonpb.DataBlob, len(m))
	for k, v := range m {
		r[k] = cloneProto(v)
	}
	return r
}

func cloneSet[K comparable](m map[K]struct{}) map[K]struct{} {
	if m == nil {
		return nil
	}
	r := make(map[K]struct{}, len(m))
	for k := range m {
		r[k] = struct{}{}
	}
	return r
}

// The manager can roll back protobuf fields after returning, so the delegate owns every mutable leaf.
func cloneRequest(r *persistence.InternalUpdateWorkflowExecutionRequest) (*persistence.InternalUpdateWorkflowExecutionRequest, error) {
	if r.NewWorkflowSnapshot != nil || len(r.NewWorkflowNewEvents) != 0 {
		return nil, fmt.Errorf("unsupported new-workflow ownership")
	}
	m := &r.UpdateWorkflowMutation
	n := persistence.InternalWorkflowMutation{
		NamespaceID: m.NamespaceID, WorkflowID: m.WorkflowID, RunID: m.RunID,
		ExecutionInfo: cloneProto(m.ExecutionInfo), ExecutionInfoBlob: cloneProto(m.ExecutionInfoBlob),
		ExecutionState: cloneProto(m.ExecutionState), ExecutionStateBlob: cloneProto(m.ExecutionStateBlob),
		NextEventID: m.NextEventID, StartVersion: m.StartVersion, LastWriteVersion: m.LastWriteVersion, DBRecordVersion: m.DBRecordVersion,
		UpsertActivityInfos: cloneBlobs(m.UpsertActivityInfos), DeleteActivityInfos: cloneSet(m.DeleteActivityInfos),
		UpsertTimerInfos: cloneBlobs(m.UpsertTimerInfos), DeleteTimerInfos: cloneSet(m.DeleteTimerInfos),
		UpsertChildExecutionInfos: cloneBlobs(m.UpsertChildExecutionInfos), DeleteChildExecutionInfos: cloneSet(m.DeleteChildExecutionInfos),
		UpsertRequestCancelInfos: cloneBlobs(m.UpsertRequestCancelInfos), DeleteRequestCancelInfos: cloneSet(m.DeleteRequestCancelInfos),
		UpsertSignalInfos: cloneBlobs(m.UpsertSignalInfos), DeleteSignalInfos: cloneSet(m.DeleteSignalInfos),
		DeleteChasmNodes: cloneSet(m.DeleteChasmNodes), UpsertSignalRequestedIDs: cloneSet(m.UpsertSignalRequestedIDs),
		DeleteSignalRequestedIDs: cloneSet(m.DeleteSignalRequestedIDs), NewBufferedEvents: cloneProto(m.NewBufferedEvents),
		ClearBufferedEvents: m.ClearBufferedEvents, Condition: m.Condition, Checksum: cloneProto(m.Checksum),
	}
	if m.UpsertChasmNodes != nil {
		n.UpsertChasmNodes = make(map[string]persistence.InternalChasmNode, len(m.UpsertChasmNodes))
		for k, v := range m.UpsertChasmNodes {
			n.UpsertChasmNodes[k] = persistence.InternalChasmNode{Metadata: cloneProto(v.Metadata), Data: cloneProto(v.Data), CassandraBlob: cloneProto(v.CassandraBlob)}
		}
	}
	if m.Tasks != nil {
		n.Tasks = make(map[tasks.Category][]persistence.InternalHistoryTask, len(m.Tasks))
		for k, v := range m.Tasks {
			if v == nil {
				n.Tasks[k] = nil
				continue
			}
			a := make([]persistence.InternalHistoryTask, len(v))
			for i, task := range v {
				a[i] = persistence.InternalHistoryTask{Key: task.Key, Blob: cloneProto(task.Blob)}
			}
			n.Tasks[k] = a
		}
	}
	var events []*persistence.InternalAppendHistoryNodesRequest
	if r.UpdateWorkflowNewEvents != nil {
		events = make([]*persistence.InternalAppendHistoryNodesRequest, len(r.UpdateWorkflowNewEvents))
	}
	for i, v := range r.UpdateWorkflowNewEvents {
		if v == nil || v.IsNewBranch {
			return nil, fmt.Errorf("expected existing-branch history append")
		}
		events[i] = &persistence.InternalAppendHistoryNodesRequest{
			BranchToken: slices.Clone(v.BranchToken), IsNewBranch: v.IsNewBranch, Info: v.Info,
			BranchInfo: cloneProto(v.BranchInfo), TreeInfo: cloneProto(v.TreeInfo), ShardID: v.ShardID,
			Node: persistence.InternalHistoryNode{NodeID: v.Node.NodeID, TransactionID: v.Node.TransactionID, PrevTransactionID: v.Node.PrevTransactionID, Events: cloneProto(v.Node.Events)},
		}
	}
	var newEvents []*persistence.InternalAppendHistoryNodesRequest
	if r.NewWorkflowNewEvents != nil {
		newEvents = make([]*persistence.InternalAppendHistoryNodesRequest, 0)
	}
	return &persistence.InternalUpdateWorkflowExecutionRequest{
		ShardID: r.ShardID, RangeID: r.RangeID, Mode: r.Mode, ArchetypeID: r.ArchetypeID,
		UpdateWorkflowMutation: n, UpdateWorkflowNewEvents: events, NewWorkflowSnapshot: nil, NewWorkflowNewEvents: newEvents,
	}, nil
}

// Include serialized blobs omitted by the request's JSON tags; protobuf caches are not semantic content.
func normalized(v reflect.Value) any {
	if !v.IsValid() {
		return nil
	}
	if v.CanInterface() {
		if p, ok := v.Interface().(proto.Message); ok {
			if v.IsNil() {
				return nil
			}
			b, err := proto.MarshalOptions{Deterministic: true}.Marshal(p)
			if err != nil {
				panic(err)
			}
			return hex.EncodeToString(b)
		}
		if t, ok := v.Interface().(time.Time); ok {
			return t.UTC().Format(time.RFC3339Nano)
		}
	}
	switch v.Kind() {
	case reflect.Pointer, reflect.Interface:
		if v.IsNil() {
			return nil
		}
		return normalized(v.Elem())
	case reflect.Struct:
		r := map[string]any{}
		for i := range v.NumField() {
			if v.Type().Field(i).IsExported() {
				r[v.Type().Field(i).Name] = normalized(v.Field(i))
			}
		}
		return r
	case reflect.Map:
		if v.IsNil() {
			return nil
		}
		r := map[string]any{}
		iter := v.MapRange()
		for iter.Next() {
			r[fmt.Sprintf("%#v", iter.Key().Interface())] = normalized(iter.Value())
		}
		return r
	case reflect.Slice, reflect.Array:
		if v.Kind() == reflect.Slice && v.IsNil() {
			return nil
		}
		r := make([]any, v.Len())
		for i := range v.Len() {
			r[i] = normalized(v.Index(i))
		}
		return r
	default:
		return v.Interface()
	}
}

func requestDigest(r *persistence.InternalUpdateWorkflowExecutionRequest) (string, error) {
	b, err := json.Marshal(normalized(reflect.ValueOf(r)))
	if err != nil {
		return "", err
	}
	return fmt.Sprintf("%x", sha256.Sum256(b)), nil
}

func normalizedValue(r *persistence.InternalUpdateWorkflowExecutionRequest) any {
	return normalized(reflect.ValueOf(r))
}

func mutableAddresses(v reflect.Value, out map[string]bool) {
	if !v.IsValid() || v.Type() == reflect.TypeFor[time.Time]() {
		return
	}
	switch v.Kind() {
	case reflect.Interface:
		if !v.IsNil() {
			mutableAddresses(v.Elem(), out)
		}
	case reflect.Pointer:
		if !v.IsNil() {
			out[fmt.Sprintf("pointer:%s:%x", v.Type(), v.Pointer())] = true
			mutableAddresses(v.Elem(), out)
		}
	case reflect.Slice:
		if v.Len() > 0 {
			out[fmt.Sprintf("slice:%s:%x", v.Type(), v.Pointer())] = true
		}
		for i := range v.Len() {
			mutableAddresses(v.Index(i), out)
		}
	case reflect.Map:
		if v.IsNil() {
			return
		}
		out[fmt.Sprintf("map:%s:%p", v.Type(), v.UnsafePointer())] = true
		iter := v.MapRange()
		for iter.Next() {
			mutableAddresses(iter.Value(), out)
		}
	case reflect.Struct:
		for i := range v.NumField() {
			f := v.Type().Field(i)
			if f.IsExported() || f.Name == "unknownFields" {
				mutableAddresses(v.Field(i), out)
			}
		}
	}
}

func verifyOwnership(a, b *persistence.InternalUpdateWorkflowExecutionRequest) (map[string]any, error) {
	aHash, err := requestDigest(a)
	if err != nil {
		return nil, err
	}
	bHash, err := requestDigest(b)
	if err != nil {
		return nil, err
	}
	if aHash != bHash {
		return nil, fmt.Errorf("clone content differs: %s vs %s", aHash, bHash)
	}
	aa, bb := map[string]bool{}, map[string]bool{}
	mutableAddresses(reflect.ValueOf(a), aa)
	mutableAddresses(reflect.ValueOf(b), bb)
	for addr := range aa {
		if bb[addr] {
			return nil, fmt.Errorf("mutable alias: %s", addr)
		}
	}
	return map[string]any{"source_digest": aHash, "clone_digest": bHash, "source_mutable_objects": len(aa), "clone_mutable_objects": len(bb), "shared_mutable_objects": 0}, nil
}
