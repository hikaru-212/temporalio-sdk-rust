#!/usr/bin/env python3
import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parents[2]
arm, run_id = sys.argv[1:]
names = {'control': 'TestCCPOC4Control', 'a': 'TestCCPOC4CommittedUnknown', 'b': 'TestCCPOC4NotCommittedUnknown'}
name = names[arm]
output = Path(__file__).resolve().parent / run_id
output.mkdir(exist_ok=False)
cmd = ['go', 'test', '-p', '2', '-tags', 'test_dep', '-count=1', '-timeout', '3m', '-run', '^'+name+'$', '-v', './tests/ccpo_c4']
info = {'command': cmd, 'cwd': str(root), 'start_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'arm': arm, 'source_sha': subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(), 'test_sha256': hashlib.sha256((root/'tests/ccpo_c4/range_barrier_test.go').read_bytes()).hexdigest()}
(output/'test-source.go.txt').write_bytes((root/'tests/ccpo_c4/range_barrier_test.go').read_bytes())
(output/'command.json').write_text(json.dumps(info,indent=2)+'\n')
with (output/'stdout.log').open('wb') as out, (output/'stderr.log').open('wb') as err:
    proc = subprocess.run(cmd, cwd=root, stdout=out, stderr=err, env={**os.environ, 'GOMAXPROCS': '2', 'CCPO_EVIDENCE_OUTPUT': str(output)})
info.update(exit_code=proc.returncode, finish_utc=datetime.datetime.now(datetime.timezone.utc).isoformat())
(output/'result.json').write_text(json.dumps(info,indent=2)+'\n')
print(json.dumps(info,indent=2),flush=True)
print((output/'stderr.log').read_text()[-8000:],flush=True)
print((output/'stdout.log').read_text()[-2000:],flush=True)
sys.exit(proc.returncode)
