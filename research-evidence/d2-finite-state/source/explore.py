"""Exhaustive unweighted BFS for the one baseline and exactly two ablations."""
import argparse
from collections import Counter, deque
from dataclasses import dataclass, asdict
import hashlib
import json
from pathlib import Path
import time

from model import (State, Action, MODES, READ, MUTATED, COMMITTED, ABORTED, REJECTED,
                   UNKNOWN, ACTIVE, transitions, describe)
from properties import (Monitor, advance, p2a, p2b, structural_invariants,
                        transition_frame)


@dataclass(frozen=True, slots=True)
class Node:
    state: State = State()
    monitor: Monitor = Monitor()


def annotated(n: Node):
    return {'state': describe(n.state), 'monitor': asdict(n.monitor),
            'P2a': p2a(n.monitor), 'P2b': p2b(n.monitor)}


def path_to(n, parents):
    path = []
    while parents[n] is not None:
        prev, action = parents[n]
        path.append({'action': asdict(action), **annotated(n)})
        n = prev
    path.append({'action': None, **annotated(n)})
    return list(reversed(path))


def save(path, x):
    with path.open('x', encoding='utf-8') as f:
        json.dump(x, f, ensure_ascii=False, indent=2)
        f.write('\n')


def targeted_witness(name, wanted, parents, contracts):
    n = Node()
    trace = [{'action': None, **annotated(n)}]
    for kind, attempt in wanted:
        choices = [(a, s) for a, s in transitions(n.state, contracts)
                   if a.kind == kind and a.attempt == attempt]
        assert len(choices) == 1, (name, kind, attempt, choices)
        a, s = choices[0]
        nxt = Node(s, advance(n.monitor, n.state, a, s))
        assert nxt in parents, ('witness not in exhausted reachable graph', name, a)
        transition_frame(n.state, a, s)
        structural_invariants(s, nxt.monitor)
        assert p2a(nxt.monitor) and p2b(nxt.monitor)
        n = nxt
        trace.append({'action': asdict(a), **annotated(n)})
    return {'name': name, 'scope': 'abstract path embedding, not a Temporal rerun',
            'steps': len(wanted), 'path': trace}


def mappings(parents, contracts, out):
    def seq(i, *kinds): return [(k, i) for k in kinds]
    renewal = seq(-1, 'RenewBegin', 'RenewCommit', 'Acquire', 'CertifyFresh')
    start0 = seq(0, 'Submit', 'AppendPhysical', 'Begin', 'ReadSnapshot')
    retry = seq(1, 'Submit', 'AppendPhysical', 'Begin', 'ReadSnapshot', 'TryMutation',
                'Commit', 'ManagerResult', 'SendAck', 'DeliverAck')
    plans = {
        'Main': start0 + seq(0, 'ManagerUnknown') + renewal +
                seq(0, 'TryMutation', 'Abort') + retry,
        'Positive': start0 + seq(0, 'TryMutation', 'Commit', 'ManagerUnknown') +
                    renewal + seq(1, 'Submit', 'ManagerResult', 'SendAck', 'DeliverAck'),
        'Negative': seq(0, 'Submit', 'ManagerUnknown') + renewal +
                    seq(0, 'AppendPhysical', 'Begin', 'ReadSnapshot', 'Abort') + retry,
        'AckLostRetry': start0 + seq(0, 'TryMutation', 'Commit', 'ManagerResult',
                                    'SendAck', 'LoseAck', 'ObserveTimeout') +
                        seq(1, 'Submit', 'ManagerResult', 'SendAck', 'DeliverAck'),
    }
    facts = {}
    for name, plan in plans.items():
        witness = targeted_witness(name, plan, parents, contracts)
        final = witness['path'][-1]['state']
        assert final['effective_batches'] in ([0], [1])
        cert_points = [x for x in witness['path'] if x['action'] and
                       x['action']['kind'] == 'CertifyFresh']
        if name == 'Main':
            at = cert_points[0]['state']; old = at['attempts'][0]
            assert at['revision'] == 3 and at['pending_K']
            assert old['stage'] == 'read' and old['snapshot_epoch'] == 1
            assert final['attempts'][0]['error'] == 'epoch_conflict_at_mutation'
        elif name == 'Positive':
            assert cert_points[0]['state']['revision'] == 4
            assert final['attempts'][1]['stage'] == 'request_rejected'
        elif name == 'Negative':
            assert cert_points[0]['state']['attempts'][0]['stage'] == 'submitted'
            assert final['attempts'][0]['error'] == 'range_mismatch_at_read'
            assert final['attempts'][0]['snapshot_epoch'] == 2
        else:
            assert final['attempts'][0]['ack'] == 'lost'
            assert final['attempts'][0]['caller_observation'] == 'unknown'
            assert witness['path'][-4]['action']['detail'] == 'retry_after_unknown'
        save(out / ('witness-' + name + '.json'), witness)
        facts[name] = {'steps': witness['steps'], 'final_revision': final['revision'],
                       'effective_batches': final['effective_batches']}
    return facts


def explore(mode, out):
    contracts = MODES[mode]
    start = time.monotonic()
    print('EXPLORATION_START ' + mode, flush=True)
    initial = Node()
    parents = {initial: None}
    depth = {initial: 0}
    queue = deque([initial])
    transitions_count = 0
    action_counts = Counter()
    violations = Counter()
    first_bad = {}
    diagnostics = Counter()
    max_acceptances = max_revision = max_depth = max_frontier = 0
    while queue:
        n = queue.popleft()
        s, m = n.state, n.monitor
        structural_invariants(s, m)
        d = depth[n]
        max_depth = max(max_depth, d)
        max_acceptances = max(max_acceptances, len(m.acceptances))
        max_revision = max(max_revision, s.revision)
        for prop, ok in [('P2a', p2a(m)), ('P2b', p2b(m))]:
            if not ok:
                violations[prop] += 1
                first_bad.setdefault(prop, n)
        if s.certificate:
            diagnostics['certified_states'] += 1
            if any(a.stage in (READ, MUTATED) and a.read_epoch == 1 for a in s.attempts):
                diagnostics['old_transaction_open_after_certificate_states'] += 1
            if s.certificate.pending:
                diagnostics['certificate_pending_states'] += 1
            else:
                diagnostics['certificate_consumed_states'] += 1
        if any(a.stage == COMMITTED and a.caller == UNKNOWN for a in s.attempts):
            diagnostics['committed_caller_unknown_states'] += 1
        successors = list(transitions(s, contracts))
        if not successors:
            diagnostics['terminal_states'] += 1
        for action, post in successors:
            transitions_count += 1
            action_counts[action.kind + ':' + action.detail] += 1
            transition_frame(s, action, post)
            nxt = Node(post, advance(m, s, action, post))
            if nxt not in parents:
                parents[nxt] = (n, action)
                depth[nxt] = d + 1
                queue.append(nxt)
            if action.kind == 'Commit' and s.certificate and s.attempts[action.attempt].request_epoch == 2:
                diagnostics['legal_new_epoch_commits_after_certificate_edges'] += 1
            if action.kind == 'Submit' and action.detail == 'retry_after_unknown':
                diagnostics['retry_after_unknown_edges'] += 1
        max_frontier = max(max_frontier, len(queue))
    shortest = {}
    for prop, n in first_bad.items():
        trace = path_to(n, parents)
        assert len(trace) - 1 == depth[n]
        assert all(point[prop] for point in trace[:-1])
        assert not trace[-1][prop]
        save(out / ('counterexample-' + prop + '.json'), {
            'mode': mode, 'property': prop, 'shortest_unit_transition_length': len(trace)-1,
            'BFS_complete': True, 'path': trace})
        shortest[prop] = len(trace)-1
    witness_results = mappings(parents, contracts, out) if mode == 'baseline' else {}
    result = {
        'mode': mode, 'contracts': asdict(contracts), 'exhaustive': True,
        'search': 'unweighted BFS; no depth/state cap, no symmetry or partial-order reduction',
        'visited_states_including_monitors': len(parents), 'directed_transition_edges': transitions_count,
        'max_shortest_depth': max_depth, 'max_frontier': max_frontier,
        'max_acceptance_count': max_acceptances, 'max_revision': max_revision,
        'property_holds': {'P2a': 'P2a' not in first_bad, 'P2b': 'P2b' not in first_bad},
        'violating_states': dict(violations), 'shortest_counterexample_steps': shortest,
        'nonvacuity': dict(diagnostics), 'action_edge_counts': dict(sorted(action_counts.items())),
        'witness_embeddings': witness_results,
        'wall_seconds': round(time.monotonic()-start, 6),
        'Temporal_experiments_executed': 0,
    }
    save(out / 'result.json', result)
    print(json.dumps(result, ensure_ascii=False, indent=2), flush=True)
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    args.output.mkdir(exist_ok=False)
    configurations = {name: asdict(c) for name, c in MODES.items()}
    assert set(configurations) == {'baseline', 'A_no_commit_claim', 'B_no_epoch_conflict'}
    for name, field in [('A_no_commit_claim', 'atomic_claim'), ('B_no_epoch_conflict', 'epoch_conflict')]:
        changed = [key for key in configurations['baseline']
                   if configurations[name][key] != configurations['baseline'][key]]
        assert changed == [field]
    save(args.output / 'configurations.json', configurations)
    save(args.output / 'source-hashes.json', {
        p.name: hashlib.sha256(p.read_bytes()).hexdigest()
        for p in sorted(Path(__file__).parent.glob('*.py'))})
    results = {}
    for name in MODES:
        d = args.output / name
        d.mkdir()
        results[name] = explore(name, d)
    save(args.output / 'summary.json', results)


if __name__ == '__main__':
    main()
