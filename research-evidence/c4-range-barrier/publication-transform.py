#!/usr/bin/env python3
"""Produce public copies from originals; never modify or overwrite source evidence."""
from pathlib import Path
import datetime
import hashlib
import json
import re
import sys

RULES = [
    (r'/tmp/temporal-ccpo-c4-range-barrier-\d{4}-\d{2}-\d{2}', '<SERVER_CHECKOUT>'),
    (r'/tmp/temporal-ccpo-c3-commit-ambiguity-\d{4}-\d{2}-\d{2}', '<C3_SERVER_CHECKOUT>'),
    (r'/tmp/temporal-ccpo-egts-c3-checkpoint-\d{4}-\d{2}-\d{2}', '<C3_CHECKPOINT>'),
    (r'/home/[^/\s]+/oss/temporal', '<ORIGINAL_SERVER_CHECKOUT>'),
    (r'/home/[^/\s]+/go/pkg/mod', '<GO_MODULE_CACHE>'),
    (r'/tmp/test_[0-9a-f]+', '<EPHEMERAL_SQLITE_DB>'),
]

def digest(data):
    return hashlib.sha256(data).hexdigest()

def transform(relative, data):
    if relative.endswith('.pb'):
        return data, []
    text = data.decode('utf-8')
    changes = []
    for pattern, placeholder in RULES:
        text, count = re.subn(pattern, placeholder, text)
        if count:
            changes.append({'rule': pattern, 'replacement': placeholder, 'count': count})
    text, count = re.subn(r"(?m)^(\s*password:)[^\r\n]*", r"\1 '<REDACTED_TEST_PASSWORD>'", text)
    if count:
        changes.append({'rule': 'YAML password configuration value (already masked in original logs)', 'replacement': '<REDACTED_TEST_PASSWORD>', 'count': count})
    return text.encode('utf-8'), changes

if __name__ == '__main__':
    original, target = map(Path, sys.argv[1:])
    assert original.resolve() != target.resolve()
    manifest_bytes = (original/'SHA256SUMS').read_bytes()
    assert digest(manifest_bytes) == '7a86d208a336d191a2b4b457a2ca81da516949233d9134b200844220892973c5'
    manifest = dict(line.split('  ',1)[::-1] for line in manifest_bytes.decode().splitlines())
    assert len(manifest) == 88
    files = {p.relative_to(original).as_posix():p for p in original.rglob('*') if p.is_file()}
    assert set(files) == set(manifest) | {'SHA256SUMS'}
    entries = []
    for relative,p in sorted(files.items()):
        before = p.read_bytes()
        if relative in manifest:
            assert digest(before) == manifest[relative], relative
        output_name = {'README.md':'README.experiment.md','SHA256SUMS':'ORIGINAL-SHA256SUMS'}.get(relative,relative)
        after, changes = transform(relative,before)
        destination = target/output_name
        destination.parent.mkdir(parents=True,exist_ok=True)
        with destination.open('xb') as out:
            out.write(after)
        entries.append({'original_path':relative,'published_path':output_name,'original_sha256':digest(before),'published_sha256':digest(after),'original_bytes':len(before),'published_bytes':len(after),'transformations':changes})
    publication = {
        'created_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'parent_checkpoint':'632c157c02af7154e4b93541c51dc1d17141c003',
        'server_source_sha':'0d5e3848bed625907987f73d5eaae8dc77bc6590',
        'original_manifest_entries_verified':88,
        'original_manifest_sha256':digest(manifest_bytes),
        'original_archive_basename':'temporal-ccpo-c4-evidence-2026-10-23.tar.gz',
        'original_archive_sha256':'494c747abb56db1ae788b915021953f8372ec339754170b382bc4dff42a83aba',
        'original_archive_files_verified':90,
        'policy':'Public copies replace machine path prefixes, temporary database filenames and already masked YAML password values only. Original workspace/archive remain unchanged locally. Keep synthetic test tokens, all protobuf bytes, identity fields, timestamps, event/task IDs, observations, exit codes and conclusions. Historical hashes refer to originals; this mapping identifies public bytes.',
        'entries':entries,
    }
    with (target/'PUBLICATION.json').open('x') as out:
        out.write(json.dumps(publication,indent=2,ensure_ascii=False)+'\n')
    print(json.dumps({'source_files':len(entries),'changed_files':sum(bool(e['transformations']) for e in entries),'unchanged_protobufs':sum(e['original_path'].endswith('.pb') for e in entries)},indent=2))
