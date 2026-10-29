"""Verify a public evidence bundle without importing or executing its model."""
import ast
import hashlib
import json
from pathlib import Path
import re
import sys


def verify(root):
    def load(name):
        return json.loads((root / name).read_text(encoding='utf-8'))

    def digest(path):
        return hashlib.sha256(path.read_bytes()).hexdigest()

    allow = load('PUBLICATION-ALLOWLIST.json')
    prefix = allow['base_directory'] + '/'
    names = []
    for name in allow['files']:
        assert name.startswith(prefix), name
        relative = name[len(prefix):]
        assert not Path(relative).is_absolute() and '..' not in Path(relative).parts
        names.append(relative)
    assert len(names) == len(set(names))
    actual = set()
    for p in root.rglob('*'):
        assert not p.is_symlink(), p
        if p.is_file():
            actual.add(p.relative_to(root).as_posix())
    assert actual == set(names), (actual - set(names), set(names) - actual)
    hashes = {}
    for line in (root / 'SHA256SUMS').read_text().splitlines():
        h, name = line.split(maxsplit=1)
        assert name not in hashes and name in actual
        assert digest(root / name) == h, name
        hashes[name] = h
    assert set(hashes) == actual - {'SHA256SUMS'}

    expected_sources = load('run-01/results/source-hashes.json')
    assert set(expected_sources) == {'model.py', 'properties.py', 'explore.py'}
    for name, expected in expected_sources.items():
        assert digest(root / 'source' / name) == expected
    invocation = load('run-01/invocation.public.json')
    assert invocation['source_sha256'] == expected_sources
    assert invocation['publication']['historical_record_only']
    outcome = load('run-01/exit.json')
    assert outcome['exit_code'] == 0 and not outcome['timed_out']
    assert (root / 'run-01/stderr.log').read_bytes() == b''

    summary = load('run-01/results/summary.json')
    expected = {'baseline': (23385, True, True),
                'A_no_commit_claim': (23105, False, True),
                'B_no_epoch_conflict': (17371, True, False)}
    assert set(summary) == set(expected)
    configs = load('run-01/results/configurations.json')
    prereg = load('PREREGISTRATION.public.json')
    for mode, (count, a, b) in expected.items():
        result = load('run-01/results/' + mode + '/result.json')
        assert summary[mode] == result and result['exhaustive']
        assert result['visited_states_including_monitors'] == count
        assert result['property_holds'] == {'P2a': a, 'P2b': b}
        assert result['contracts'] == configs[mode]
        assert result['Temporal_experiments_executed'] == 0
    assert len(prereg['configurations']) == 3
    for mode, changed in [('A_no_commit_claim', 'atomic_claim'),
                          ('B_no_epoch_conflict', 'epoch_conflict')]:
        assert [key for key in configs['baseline']
                if configs[mode][key] != configs['baseline'][key]] == [changed]

    raw = (root / 'run-01/stdout.log').read_text()
    decoder = json.JSONDecoder()
    for mode in expected:
        marker = 'EXPLORATION_START ' + mode + '\n'
        assert raw.startswith(marker)
        raw = raw[len(marker):].lstrip()
        obj, end = decoder.raw_decode(raw)
        assert obj == summary[mode]
        raw = raw[end:].lstrip()
    assert not raw

    traces = {}
    for p in sorted((root / 'run-01/results').glob('*/*.json')):
        record = json.loads(p.read_text())
        if 'path' not in record:
            continue
        accepts, late, cert = [], [], None
        for point in record['path']:
            action, state = point['action'], point['state']
            if action and action['kind'] == 'Commit':
                i = action['attempt']
                accepts.append(i)
                if cert and state['attempts'][i]['request_epoch'] < cert['epoch']:
                    late.append(i)
            if action and action['kind'] == 'CertifyFresh':
                cert = state['certificate']
                assert cert['epoch'] == state['epoch'] == 2
                for key in ('revision', 'pending_K', 'effective_batches', 'durable_tasks'):
                    assert cert[key] == state[key]
            assert point['monitor']['acceptances'] == accepts
            assert point['monitor']['post_certificate_old_commits'] == late
            assert point['P2a'] == (len(accepts) <= 1)
            assert point['P2b'] == (len(late) == 0)
            assert state['effective_batches'] == state['durable_tasks'] == accepts
            assert state['revision'] == 3 + len(accepts)
        if 'property' in record:
            prop = record['property']
            assert all(x[prop] for x in record['path'][:-1])
            assert not record['path'][-1][prop]
            assert len(record['path']) - 1 == record['shortest_unit_transition_length']
        traces[p.relative_to(root).as_posix()] = len(record['path']) - 1
    assert len(traces) == 6
    assert traces['run-01/results/A_no_commit_claim/counterexample-P2a.json'] == 12
    assert traces['run-01/results/B_no_epoch_conflict/counterexample-P2b.json'] == 10

    mapping = load('ORIGINAL-TO-PUBLIC.json')
    for item in mapping['original_files']:
        for target in item['public_files']:
            p = root / target['path']
            assert digest(p) == target['sha256'] and p.stat().st_size == target['bytes']
            if target['transformation'] == 'exact-copy':
                assert target['sha256'] == item['original_sha256']
    original_manifest = (root / 'provenance/original-SHA256SUMS').read_text().splitlines()
    assert len(original_manifest) == 37
    original_hashes = dict((name, h) for h, name in
                           (line.split(maxsplit=1) for line in original_manifest))
    assert set(original_hashes) == {x['original_path'] for x in mapping['original_files']} - {'SHA256SUMS'}
    for item in mapping['original_files']:
        if item['original_path'] != 'SHA256SUMS':
            assert original_hashes[item['original_path']] == item['original_sha256']

    dependency_graph = {}
    for p in sorted((root / 'source').glob('*.py')):
        tree = ast.parse(p.read_text(), filename=p.name)
        dependencies = set()
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                dependencies.update(a.name.split('.')[0] for a in node.names)
            elif isinstance(node, ast.ImportFrom):
                assert node.level == 0 and node.module
                dependencies.add(node.module.split('.')[0])
            elif isinstance(node, ast.Call) and isinstance(node.func, ast.Name):
                assert node.func.id not in {'eval', 'exec', 'compile', '__import__'}
        assert dependencies <= set(sys.stdlib_module_names) | {'model', 'properties'}
        assert not dependencies & {'subprocess', 'socket', 'urllib', 'requests'}
        dependency_graph[p.name] = sorted(dependencies)

    patterns = {
        'absolute_private_path': r'/(?:home|Users|tmp|var/tmp|dev/mapper)/',
        'session_or_internal_ref': r'(?:refs/[c]odex/|[.]codex/sessions|rollout-\d{4})',
        'private_key': r'-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----',
        'github_token': r'(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{30,})',
        'aws_access_key': r'AKIA[0-9A-Z]{16}',
        'jwt': r'eyJ[A-Za-z0-9_-]{16,}\.[A-Za-z0-9_-]{16,}\.[A-Za-z0-9_-]{16,}',
    }
    for name in sorted(actual):
        content = (root / name).read_bytes()
        assert b'\0' not in content, name
        text = content.decode('utf-8')
        for label, pattern in patterns.items():
            assert not re.search(pattern, text), (name, label)
    for p in root.rglob('*.md'):
        for target in re.findall(r'\]\(([^)]+)\)', p.read_text()):
            if '://' not in target and not target.startswith('#'):
                assert (p.parent / target.split('#', 1)[0]).exists(), (p.name, target)

    return {'verification': 'PASS', 'public_files': len(actual),
            'bytes': sum((root / n).stat().st_size for n in actual),
            'manifest_entries': len(hashes), 'manifest_sha256': digest(root / 'SHA256SUMS'),
            'source_hashes': expected_sources, 'saved_trace_lengths': traces,
            'static_dependencies': dependency_graph,
            'model_executed': False, 'model_imported': False,
            'reproducibility_scope': 'Static dependency closure and exact historic-source hashes; no new model run',
            'privacy_scope': 'Explicit allowlist, UTF-8/text inspection and pattern scan; not a universal secret detector'}


if __name__ == '__main__':
    root = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path(__file__).resolve().parents[1]
    print(json.dumps(verify(root), ensure_ascii=False, indent=2))
