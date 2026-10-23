#!/usr/bin/env python3
"""Verify the published evidence bundle without running Server or Go tests."""
import hashlib
import json
from pathlib import Path

root = Path(__file__).resolve().parent

def read(relative):
    path = (root / relative).resolve()
    if not path.is_relative_to(root):
        raise ValueError(f"path escapes bundle: {relative}")
    return path.read_bytes()

def digest(data):
    return hashlib.sha256(data).hexdigest()

manifest = {}
for line in read('SHA256SUMS').decode().splitlines():
    sha, name = line.split('  ', 1)
    assert name not in manifest, name
    assert digest(read(name)) == sha, name
    manifest[name] = sha
actual = {p.relative_to(root).as_posix() for p in root.rglob('*') if p.is_file()}
assert actual == set(manifest) | {'SHA256SUMS'}, 'unlisted or missing files'
publication = json.loads(read('PUBLICATION.json'))
entries = {e['original_path']: e for e in publication['entries']}
for entry in entries.values():
    data = read(entry['published_path'])
    assert digest(data) == entry['published_sha256'], entry['published_path']
    assert len(data) == entry['published_bytes']
original_manifest = read('ORIGINAL-SHA256SUMS')
assert digest(original_manifest) == publication['original_manifest_sha256']
for line in original_manifest.decode().splitlines():
    sha, name = line.split('  ', 1)
    assert entries[name]['original_sha256'] == sha, name
assert len(original_manifest.decode().splitlines()) == 58

final_source = read('server-test/commit_ambiguity_test.go')
v1_source = read('harness-v1.test.go.txt')
for diff_name, expected in [('test-only.diff', final_source), ('harness-v1.diff', v1_source)]:
    added = b''.join(line[1:] for line in read(diff_name).splitlines(keepends=True)
                     if line.startswith(b'+') and not line.startswith(b'+++ '))
    assert added == expected, diff_name

prereg = json.loads(read('preregistration.json'))
assert digest(read('PREDICTIONS.md')) == prereg['predictions_sha256']
runs = sorted(p for p in root.glob('run-*') if p.is_dir())
assert len(runs) == 8
for run in runs:
    name = run.name
    result = json.loads(read(f'{name}/result.json'))
    invocation = json.loads(read(f'{name}/command.json'))
    assert all(result[k] == value for k, value in invocation.items())
    assert result['source_sha'] == publication['server_source_sha']
    source = v1_source if int(name.split('-')[1]) <= 4 else final_source
    assert result['test_sha256'] == digest(source)
    assert result['exit_code'] == 0
    assert prereg['registered_utc'] < result['start_utc']
    assert read(f'{name}/stderr.log') == b''
    observations = json.loads(read(f'{name}/observations.json'))
    assert observations['stdout_sha256'] == entries[f'{name}/stdout.log']['original_sha256']
    lines = read(f'{name}/stdout.log').decode().splitlines()
    actual_observations = [json.loads(line.split('CCPO ', 1)[1]) for line in lines if 'CCPO {' in line]
    expected = []
    for record in observations['records']:
        event = {k: v for k, v in record.items() if k != 'stdout_line'}
        assert json.loads(lines[record['stdout_line'] - 1].split('CCPO ', 1)[1]) == event
        expected.append(event)
    assert actual_observations == expected
    assert '\nPASS\n' in '\n'.join(lines) + '\n'
assert read('lint-check/exit-code.txt').strip() == b'2'
print(f'PASS: {len(manifest)} public file hashes, 58 original hash references, 8 historical runs, both test patches; lint remains exit 2. No experiment executed.')
