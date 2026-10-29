"""D-2 split-contract transition system; standard library, no Temporal execution.

Safety predicates and historical monitors deliberately live in properties.py.
Resource bounds are two attempt slots and one epoch renewal, not an acceptance
limit. Committing installs a whole batch atomically, even in the claim ablation.
"""
from dataclasses import dataclass, replace
from typing import Iterator

UNUSED, SUBMITTED, APPENDED, BEGUN, READ, MUTATED, FAILED, ABORTED, COMMITTED, REJECTED = range(10)
STAGES = ('unused', 'submitted', 'appended', 'begun', 'read', 'mutated',
          'failed', 'aborted', 'committed', 'request_rejected')
WAITING, UNKNOWN, SUCCESS, FAILURE = range(4)
OBS = ('waiting', 'unknown', 'success', 'failure')
NO_ACK, SENT, DELIVERED, LOST = range(4)
ACKS = ('none', 'sent', 'delivered', 'lost')
ACTIVE, RENEWING, ACQUIRING = range(3)
AUTHS = ('active', 'renewing', 'acquiring')


@dataclass(frozen=True, slots=True)
class Contracts:
    atomic_claim: bool = True
    epoch_conflict: bool = True


MODES = {
    'baseline': Contracts(),
    'A_no_commit_claim': Contracts(atomic_claim=False),
    'B_no_epoch_conflict': Contracts(epoch_conflict=False),
}


@dataclass(frozen=True, slots=True)
class Attempt:
    stage: int = UNUSED
    request_epoch: int = 0
    request_revision: int = 0
    read_epoch: int = 0
    read_revision: int = 0
    read_pending: bool = False
    error: str = ''
    manager: int = WAITING
    caller: int = WAITING
    ack: int = NO_ACK


@dataclass(frozen=True, slots=True)
class Certificate:
    epoch: int
    revision: int
    pending: bool
    effective_batches: tuple[int, ...]
    tasks: tuple[int, ...]


@dataclass(frozen=True, slots=True)
class State:
    epoch: int = 1
    authority: int = ACTIVE
    pending: bool = True
    revision: int = 3
    physical_appends: frozenset[int] = frozenset()
    effective_batches: tuple[int, ...] = ()
    tasks: tuple[int, ...] = ()
    attempts: tuple[Attempt, Attempt] = (Attempt(), Attempt())
    certificate: Certificate | None = None


@dataclass(frozen=True, slots=True)
class Action:
    kind: str
    attempt: int = -1
    detail: str = ''


def with_attempt(s: State, i: int, a: Attempt, **changes) -> State:
    attempts = list(s.attempts)
    attempts[i] = a
    return replace(s, attempts=tuple(attempts), **changes)


def transitions(s: State, c: Contracts) -> Iterator[tuple[Action, State]]:
    for i, a in enumerate(s.attempts):
        if a.stage == UNUSED:
            if s.authority != ACTIVE or (i == 1 and s.attempts[0].stage == UNUSED):
                continue
            prior = s.attempts[0]
            why = ('first' if i == 0 else 'retry_after_unknown' if prior.caller == UNKNOWN
                   else 'duplicate_after_reply' if prior.caller in (SUCCESS, FAILURE)
                   else 'concurrent')
            n = replace(a, request_epoch=s.epoch, request_revision=s.revision,
                        stage=SUBMITTED if s.pending else REJECTED,
                        error='' if s.pending else 'current_task_absent')
            yield Action('Submit', i, why), with_attempt(s, i, n)
            continue

        if a.stage == SUBMITTED:
            yield Action('AppendPhysical', i), with_attempt(
                s, i, replace(a, stage=APPENDED),
                physical_appends=s.physical_appends | {i})
        elif a.stage == APPENDED:
            yield Action('Begin', i), with_attempt(s, i, replace(a, stage=BEGUN))
        elif a.stage == BEGUN:
            error = ('range_mismatch_at_read' if a.request_epoch != s.epoch else
                     'claim_mismatch_at_read' if not s.pending or a.request_revision != s.revision
                     else '')
            n = replace(a, read_epoch=s.epoch, read_revision=s.revision,
                        read_pending=s.pending, stage=FAILED if error else READ, error=error)
            yield Action('ReadSnapshot', i, error or 'validated'), with_attempt(s, i, n)
        elif a.stage == READ:
            # This is the same epoch read-set contract checked at write admission
            # and commit; there is deliberately no whole-database snapshot guard.
            error = 'epoch_conflict_at_mutation' if c.epoch_conflict and a.read_epoch != s.epoch else ''
            yield Action('TryMutation', i, error or 'private_write_set'), with_attempt(
                s, i, replace(a, stage=FAILED if error else MUTATED, error=error))
        elif a.stage == MUTATED:
            error = ('epoch_conflict_at_commit' if c.epoch_conflict and a.read_epoch != s.epoch else
                     'claim_conflict_at_commit' if c.atomic_claim and
                     (not s.pending or a.read_revision != s.revision) else '')
            if error:
                yield Action('CommitRejected', i, error), with_attempt(
                    s, i, replace(a, stage=FAILED, error=error))
            else:
                yield Action('Commit', i), with_attempt(
                    s, i, replace(a, stage=COMMITTED), pending=False,
                    revision=s.revision + 1,
                    effective_batches=s.effective_batches + (i,), tasks=s.tasks + (i,))
        elif a.stage == FAILED:
            yield Action('Abort', i, a.error), with_attempt(s, i, replace(a, stage=ABORTED))

        if a.manager == WAITING:
            # Controlled manager uncertainty can precede the delegate or follow
            # its commit, without cancelling or resolving the transaction.
            if a.stage != REJECTED:
                yield Action('ManagerUnknown', i), with_attempt(
                    s, i, replace(a, manager=UNKNOWN, caller=UNKNOWN))
            if a.stage in (COMMITTED, ABORTED, REJECTED):
                result = SUCCESS if a.stage == COMMITTED else FAILURE
                yield Action('ManagerResult', i, OBS[result]), with_attempt(
                    s, i, replace(a, manager=result))
        if a.manager in (SUCCESS, FAILURE) and a.ack == NO_ACK:
            yield Action('SendAck', i, OBS[a.manager]), with_attempt(s, i, replace(a, ack=SENT))
        if a.ack == SENT:
            yield Action('DeliverAck', i, OBS[a.manager]), with_attempt(
                s, i, replace(a, ack=DELIVERED, caller=a.manager))
            yield Action('LoseAck', i), with_attempt(s, i, replace(a, ack=LOST))
        if a.caller == WAITING:
            yield Action('ObserveTimeout', i), with_attempt(s, i, replace(a, caller=UNKNOWN))

    # Authority rotation is independent of whether any particular WFT committed.
    if s.epoch == 1 and s.authority == ACTIVE:
        yield Action('RenewBegin'), replace(s, authority=RENEWING)
    elif s.epoch == 1 and s.authority == RENEWING:
        yield Action('RenewCommit'), replace(s, epoch=2, authority=ACQUIRING)
    elif s.epoch == 2 and s.authority == ACQUIRING:
        yield Action('Acquire'), replace(s, authority=ACTIVE)
    if s.epoch == 2 and s.authority == ACTIVE and s.certificate is None:
        cert = Certificate(s.epoch, s.revision, s.pending, s.effective_batches, s.tasks)
        yield Action('CertifyFresh'), replace(s, certificate=cert)


def describe(s: State) -> dict:
    return {
        'K': 'fixed-normal-WFT', 'payload': 'fixed-ScheduleActivityTask',
        'epoch': s.epoch, 'authority': AUTHS[s.authority], 'pending_K': s.pending,
        'revision': s.revision, 'physical_appends': sorted(s.physical_appends),
        'effective_batches': list(s.effective_batches), 'durable_tasks': list(s.tasks),
        'certificate': None if s.certificate is None else {
            'epoch': s.certificate.epoch, 'revision': s.certificate.revision,
            'pending_K': s.certificate.pending,
            'effective_batches': list(s.certificate.effective_batches),
            'durable_tasks': list(s.certificate.tasks)},
        'attempts': [{
            'id': i, 'stage': STAGES[a.stage], 'request_epoch': a.request_epoch,
            'request_revision': a.request_revision, 'snapshot_epoch': a.read_epoch,
            'snapshot_revision': a.read_revision, 'snapshot_pending': a.read_pending,
            'error': a.error, 'manager_result': OBS[a.manager],
            'caller_observation': OBS[a.caller], 'ack': ACKS[a.ack],
        } for i, a in enumerate(s.attempts)],
    }
