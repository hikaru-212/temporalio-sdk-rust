#!/usr/bin/env python3
"""Audit saved evidence without running Go or mutating experimental outputs."""
from pathlib import Path
import json, hashlib
base = Path(__file__).resolve().parent
runs = ['run-03-control', 'run-04-committed-unknown', 'run-05-not-committed-unknown']
summary = {}
for name in runs:
    d = base / name
    records = sorted([json.loads(line.split('CCPO ', 1)[1]) for line in (d/'stdout.log').read_text().splitlines() if 'CCPO {' in line], key=lambda x: x['sequence'])
    by = {r['stage']:r['value'] for r in records}
    assert json.loads((d/'result.json').read_text())['exit_code'] == 0
    for r in records:
        if r['stage'] == 'protobuf-artifact':
            v = r['value']; assert hashlib.sha256((d/v['file']).read_bytes()).hexdigest() == v['sha256']
    def ledger(state):
        result = {}
        assert set(state['independent_category_ranges']) == {'transfer','timer','visibility','replication','archival','outbound'}
        for cat, data in state['independent_category_ranges'].items():
            assert data['final_page_token_empty']
            tasks = data['matching_workflow_tasks'] or []
            assert data['rows'] == len(tasks), 'extra workflow task rows must be reviewed'
            keys = [(x['TaskID'], x['VisibilityTimestamp']) for x in tasks]
            assert len(keys) == len(set(keys))
            result[cat] = {x['TaskID']:x for x in tasks}
        return result
    initial = ledger(by['initial'])
    final_state = by['committed' if name == runs[0] else 'after-identical-retry']
    final = ledger(final_state)
    added = {cat:[v for key,v in rows.items() if key not in initial[cat]] for cat,rows in final.items()}
    assert {cat:len(v) for cat,v in added.items()} == {'archival':0,'outbound':0,'replication':0,'timer':1,'transfer':1,'visibility':1}
    assert added['transfer'][0]['ScheduledEventID'] == 5
    assert added['timer'][0]['EventID'] == 5
    assert final_state['valid_completions'] == final_state['activity_schedules'] == 1
    assert final_state['db_revision'] == 4
    events = final_state['effective_history']['events']
    assert [int(x['event_id']) for x in events] == [1,2,3,4,5]
    assert by['recovery-poll']['external_effect_executed'] is False
    assert by['recovery-token']['scheduled_event_id'] == '5'
    extra = {}
    if name != runs[0]:
        assert by['durable-range-barrier'] == {'durable_new_range':2,'old_range':1}
        assert by['completion-1']['error_type'] == '*serviceerror.DeadlineExceeded'
        assert by['injection-entry']['write_range'] == 1
        assert 'shard.(*ContextImpl).UpdateWorkflowExecution' in by['injection-entry']['stack']
        assert 'shard.(*ContextImpl).renewRangeLocked' in by['reacquisition-enter']['stack']
        assert 'shard.(*ContextImpl).acquireShard' in by['reacquisition-enter']['stack']
        loads = [r['value'] for r in records if r['stage']=='history-manager-reload']
        assert loads and all(v['phase']==2 and 'shard.(*ContextImpl).GetWorkflowExecution' in v['stack'] for v in loads)
        middle = ledger(by['after-durable-range-barrier'])
        if name == runs[1]:
            assert by['delegate-result']['invoked'] is True
            assert middle == final
            assert by['after-durable-range-barrier']['state'] == final_state['state']
            assert by['identical-retry']['error_type'] == '*serviceerror.NotFound'
        else:
            assert by['delegate-result']['invoked'] is False
            assert by['before-fault']['state'] == by['after-durable-range-barrier']['state']
            assert middle == initial
            assert by['identical-retry']['received_success'] is True
        extra['reload_count'] = len(loads)
    summary[name] = {'classification':'CONFIRMED SAFETY in controlled SQLite scope','added_task_ids':{k:[x['TaskID'] for x in v] for k,v in added.items()},'request_sha256':by['request']['sha256'],'final_revision':4,'valid_acceptances':1,'task_categories_independently_enumerated':6,**extra}
print(json.dumps(summary,ensure_ascii=False,indent=2))
