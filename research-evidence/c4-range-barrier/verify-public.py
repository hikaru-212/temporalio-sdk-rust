#!/usr/bin/env python3
"""Verify checkpoint evidence only; never run Server, compile, or execute Go tests."""
from pathlib import Path
import base64
import hashlib
import json

root = Path(__file__).resolve().parent

def read(name):
    p = (root/name).resolve()
    assert p.is_relative_to(root), name
    return p.read_bytes()

def sha(data):
    return hashlib.sha256(data).hexdigest()

manifest = {}
for line in read('SHA256SUMS').decode().splitlines():
    h,name = line.split('  ',1)
    assert name not in manifest
    assert sha(read(name)) == h, name
    manifest[name] = h
assert {p.relative_to(root).as_posix() for p in root.rglob('*') if p.is_file()} == set(manifest) | {'SHA256SUMS'}
publication = json.loads(read('PUBLICATION.json'))
assert publication['parent_checkpoint'] == '632c157c02af7154e4b93541c51dc1d17141c003'
entries = {e['original_path']:e for e in publication['entries']}
assert len(entries) == 89
for entry in entries.values():
    data = read(entry['published_path'])
    assert sha(data) == entry['published_sha256']
    assert len(data) == entry['published_bytes']
    if not entry['transformations']:
        assert entry['original_sha256'] == entry['published_sha256']
original_manifest = read('ORIGINAL-SHA256SUMS')
assert sha(original_manifest) == publication['original_manifest_sha256']
for line in original_manifest.decode().splitlines():
    h,name = line.split('  ',1)
    assert entries[name]['original_sha256'] == h
assert len(original_manifest.decode().splitlines()) == 88
assert sum(bool(x['transformations']) for x in entries.values()) == 21
assert sum(x['original_path'].endswith('.pb') for x in entries.values()) == 38
for entry in entries.values():
    if entry['original_path'].endswith('.pb'):
        assert entry['original_sha256'] == entry['published_sha256']
source = read('server-test/range_barrier_test.go.txt')
assert sha(source) == 'a6fd789b274808c0a807555795ee3598de935694c64bec6af26202a795f02223'
patch = read('test-only.diff')
assert b'diff --git a/tests/ccpo_c4/range_barrier_test.go b/tests/ccpo_c4/range_barrier_test.go' in patch
assert b''.join(x[1:] for x in patch.splitlines(keepends=True) if x.startswith(b'+') and not x.startswith(b'+++ ')) == source
prereg = json.loads(read('preregistration.json'))
assert sha(read('PREDICTIONS.zh-TW.md')) == prereg['predictions_sha256']
assert prereg['test_source_exists'] is False
expected = {'run-01-control':1,'run-02-control':1,'run-03-control':0,'run-04-committed-unknown':0,'run-05-not-committed-unknown':0}
assert {p.name for p in root.glob('run-*') if p.is_dir()} == set(expected)
for run,code in expected.items():
    invocation = json.loads(read(run+'/command.json'))
    result = json.loads(read(run+'/result.json'))
    assert all(result[k]==v for k,v in invocation.items())
    assert result['exit_code'] == code
    assert result['source_sha'] == publication['server_source_sha']
    assert prereg['utc'] < result['start_utc']
    assert sha(read(run+'/test-source.go.txt')) == result['test_sha256']
    if code == 0:
        assert read(run+'/test-source.go.txt') == source
    lines = read(run+'/stdout.log').decode().splitlines()
    observations = sorted([json.loads(line.split('CCPO ',1)[1]) for line in lines if 'CCPO {' in line], key=lambda x:x['sequence'])
    assert observations == json.loads(read(run+'/observations.json'))
    assert read(run+'/stderr.log') == b''
    if code == 0:
        assert '\nPASS\n' in '\n'.join(lines)+'\n'
    else:
        assert 'INCONCLUSIVE' in read(run+'/CLASSIFICATION.zh-TW.md').decode()
    for observation in observations:
        if observation['stage'] == 'protobuf-artifact':
            value = observation['value']
            assert sha(read(run+'/'+value['file'])) == value['sha256']
    request = next(x['value'] for x in observations if x['stage']=='request')
    assert request['request']['namespace'].startswith('ccpo-c4-')
    assert request['token']['workflow_id'].startswith('ccpo-c4-')
    assert bytes.fromhex(request['token_hex']) == read(run+'/task-token.pb')
    assert base64.b64decode(request['request']['task_token']) == read(run+'/task-token.pb')
    assert sha(read(run+'/request.pb')) == request['sha256']
assert json.loads(read('lint-check/result.json'))['exit_code'] == 2
print(f'PASS: {len(manifest)} public hashes; 88 original manifest entries; 89 original/public mappings; 38 byte-identical protobufs; five historical runs; exact source/patch; lint remains exit 2. No experiment executed.')
