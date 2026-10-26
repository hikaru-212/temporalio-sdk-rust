"""Offline evidence validation only: no compilation, database/service calls, or experiments."""
import hashlib
import json
from pathlib import Path
from collections import defaultdict

root = Path(__file__).resolve().parents[1]
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def read(path): return json.loads((root/path).read_text())
allow=read('PUBLICATION-ALLOWLIST.json')['files']
assert {str(p.relative_to(root)) for p in root.rglob('*') if p.is_file()}=={x['path'] for x in allow}
assert not any(p.is_symlink() for p in root.rglob('*'))
manifest=[]
for line in (root/'SHA256SUMS').read_text().splitlines():
    expected,path=line.split('  ',1);assert sha(root/path)==expected,path;manifest.append(path)
assert set(manifest)=={x['path'] for x in allow}-{'SHA256SUMS'}
transforms=read('PUBLIC-TRANSFORMATIONS.json')
for item in transforms['files']:
    assert sha(root/item['public_path'])==item['public_sha256'],item['public_path']
expected=read('PROVENANCE.json')['test_source_sha256']
for name,h in expected.items():assert sha(root/'source/tests/ccpo_c5b2'/name)==h
counts={}
for arm in ['main','positive','negative']:
    base='runs/'+arm+'/'
    ev=read(base+'events.public.json')['events'];by=defaultdict(list)
    assert [x['sequence'] for x in ev]==list(range(1,len(ev)+1))
    for e in ev:by[e['stage']].append(e)
    def val(stage):assert len(by[stage])==1;return by[stage][0]['value']
    def seq(stage):assert len(by[stage])==1;return by[stage][0]['sequence']
    res=read(base+'result.public.json');assert res['original_exit_code']==0 and res['classification']==arm.title()+' PASS'
    assert val('G-real-range-renewal-return')=={'previous':1,'new':2,'error':'<nil>'}
    assert val('independent-durable-range-read')['range']==2
    assert val('delegate-terminated')['context_before_cancel']=='<nil>'
    assert val('normal-Activity-poll')['external_effect_executed'] is False
    assert val('normal-Activity-poll')['error']=='<nil>'
    assert val('connection-hook-unregistered')['remaining_registry_entries']==0
    joins=[e['sequence'] for e in by['cleanup-joined']]
    assert seq('delegate-context-cancelled')<min(joins)<max(e['sequence'] for e in by['physical-driver-close'])
    assert max(joins)<min(e['sequence'] for e in by['physical-driver-close'])
    assert all(e['value']['error']=='<nil>' for e in by['physical-driver-close'])
    if arm=='main':
        pre,post,final='before-old-release','J-after-old-termination','L-after-legitimate-retry'
        assert seq('B-real-shard-read')<seq('G-real-range-renewal-return')<seq('I-guarded-fresh-MS-read')<seq('open-old-transaction-probe')<seq('G-release-old-transaction')<seq('first-SQL-mutation')<seq('real-tx-rollback-return')
        assert val('open-old-transaction-probe')['range']==1 and not val('open-old-transaction-probe')['released']
        assert val('raw-SQL-result')['sqlite_code']==517 and not by['real-tx-commit-return']
    elif arm=='positive':
        pre,post,final='positive-after-renewal','positive-after-rejected-retry','positive-after-rejected-retry'
        assert seq('real-tx-commit-return')<seq('delegate-terminated')<seq('injected-store-timeout')<seq('G-real-range-renewal-return')<seq('I-guarded-fresh-MS-read')
        assert val('real-tx-commit-return')['error']=='<nil>' and not by['real-tx-rollback-return']
        assert val('positive-identical-retry')['error_type']=='*serviceerror.NotFound'
    else:
        pre,post,final='negative-before-old-launch','negative-after-stale-attempt','negative-after-legitimate-retry'
        assert seq('injected-store-timeout-before-delegate')<seq('G-real-range-renewal-return')<seq('I-guarded-fresh-MS-read')<seq('delegate-invocation')<seq('B-real-shard-read')<seq('real-tx-rollback-return')
        assert val('delegate-invocation')['range']==1 and val('B-real-shard-read')['range']==2
        assert val('delegate-terminated')['type']=='*persistence.ShardOwnershipLostError'
        assert not by['first-SQL-mutation'] and not by['real-tx-commit-return']
    for k in ['range_id','db_revision','state','effective_history','independent_category_ranges']:assert val(pre)[k]==val(post)[k]
    assert val(final)['db_revision']==4 and val(final)['valid_completions']==val(final)['activity_schedules']==1
    assert len(val(final)['direct_persisted_activity_tasks'])==1
    pages=defaultdict(list);n=0
    for e in ev:
        v=e['value']
        if e['stage']=='task-range-page':pages[v['category']].append(v);n+=1
        elif 'independent_category_ranges' in v:
            assert set(pages)=={'transfer','timer','visibility','replication','archival','outbound'}
            for category,ledger in v['independent_category_ranges'].items():
                token=None;tasks=[]
                for i,page in enumerate(pages[category],1):
                    assert page['page']==i and page['request_token']==token
                    assert page['rows']==len(page['tasks'] or [])
                    token=page['next_token'];tasks+=page['tasks'] or []
                assert not token and len(tasks)==ledger['rows'] and len(pages[category])==ledger['pages']
                assert len({x['TaskID'] for x in tasks})==len(tasks)
            hist=v['effective_history']['events']
            assert [int(x['event_id']) for x in hist]==list(range(1,int(v['state']['next_event_id'])))
            com=[x for x in hist if x['event_type']=='EVENT_TYPE_WORKFLOW_TASK_COMPLETED']
            acts=[x for x in hist if x['event_type']=='EVENT_TYPE_ACTIVITY_TASK_SCHEDULED']
            assert len(com)==v['valid_completions']<=1 and len(acts)==v['activity_schedules']<=1
            if com:
                assert com[0]['event_id']=='4' and com[0]['workflow_task_completed_event_attributes']['scheduled_event_id']=='2' and com[0]['workflow_task_completed_event_attributes']['started_event_id']=='3'
                assert acts[0]['event_id']=='5' and acts[0]['activity_task_scheduled_event_attributes']['workflow_task_completed_event_id']=='4'
            pages=defaultdict(list)
    assert not pages
    counts[arm]={'events':len(ev),'task_pages':n,'valid_acceptances':val(final)['valid_completions']}
print(json.dumps({'offline_public_evidence_validation':'PASS','files':len(allow),'manifest_entries':len(manifest),'runs':counts,'experiments_executed':0},indent=2))
