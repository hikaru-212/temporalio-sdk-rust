"""Trace properties and audit monitors, separate from the transition generator."""
from dataclasses import dataclass
from model import (State, Action, COMMITTED, SUCCESS, DELIVERED, READ, MUTATED,
                   ACTIVE)


@dataclass(frozen=True, slots=True)
class Monitor:
    acceptances: tuple[int, ...] = ()
    post_certificate_old_commits: tuple[int, ...] = ()


def effective(s: State):
    return s.pending, s.revision, s.effective_batches, s.tasks


def advance(m: Monitor, pre: State, action: Action, post: State) -> Monitor:
    if action.kind != 'Commit':
        return m
    i = action.attempt
    late = (pre.certificate is not None and
            pre.attempts[i].request_epoch < pre.certificate.epoch and
            effective(pre) != effective(post))
    return Monitor(m.acceptances + (i,),
                   m.post_certificate_old_commits + ((i,) if late else ()))


def p2a(m: Monitor) -> bool:
    return len(m.acceptances) <= 1


def p2b(m: Monitor) -> bool:
    return not m.post_certificate_old_commits


def structural_invariants(s: State, m: Monitor) -> None:
    assert s.revision == 3 + len(m.acceptances)
    assert s.pending == (len(m.acceptances) == 0)
    assert s.effective_batches == s.tasks == m.acceptances
    assert len(set(m.acceptances)) == len(m.acceptances)
    assert set(m.acceptances) <= s.physical_appends
    assert {i for i, a in enumerate(s.attempts) if a.stage == COMMITTED} == set(m.acceptances)
    for a in s.attempts:
        if a.manager == SUCCESS or (a.ack == DELIVERED and a.caller == SUCCESS):
            assert a.stage == COMMITTED
        if a.stage in (READ, MUTATED, COMMITTED):
            assert a.read_pending and a.read_epoch == a.request_epoch
            assert a.read_revision == a.request_revision
    if s.certificate:
        assert s.epoch == s.certificate.epoch == 2 and s.authority == ACTIVE


def transition_frame(pre: State, action: Action, post: State) -> None:
    if action.kind != 'Commit':
        assert effective(pre) == effective(post)
    else:
        assert post.effective_batches == pre.effective_batches + (action.attempt,)
        assert post.tasks == pre.tasks + (action.attempt,)
    if action.kind == 'CertifyFresh':
        c = post.certificate
        assert c is not None and c.epoch == pre.epoch == 2
        assert (c.pending, c.revision, c.effective_batches, c.tasks) == effective(pre)
