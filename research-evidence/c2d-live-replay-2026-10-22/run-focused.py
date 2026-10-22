import datetime, json, os, pathlib, selectors, subprocess, sys
root=pathlib.Path(__file__).resolve().parent
registration=json.loads((root/'predictions.before-run.json').read_text())
run=root/sys.argv[1]
run.mkdir(exist_ok=False)
fixtures=run/'fixtures'
fixtures.mkdir()
env=os.environ.copy()
env.update({'C2D_OUTPUT_DIR':str(fixtures),'C2D_ORIGINAL_E2':registration['original_E2'],'TEMPORAL_USE_WFT_CHUNKING_V2':'1','CARGO_TARGET_DIR':'__BUILD_CACHE__','CARGO_BUILD_JOBS':'2','CARGO_PROFILE_DEV_DEBUG':'0','CARGO_PROFILE_TEST_DEBUG':'0','CARGO_TERM_COLOR':'never'})
command=['cargo','test','--locked','--offline','-p','temporalio-sdk-core','--lib','worker::workflow::managed_run::c2d_live_replay::live_replay_semantic_convergence','--','--exact','--nocapture']
record={'command':command,'cwd':registration['workspace'],'source_sha':registration['source_sha'],'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'environment_overrides':{k:env[k] for k in ['C2D_OUTPUT_DIR','C2D_ORIGINAL_E2','TEMPORAL_USE_WFT_CHUNKING_V2','CARGO_TARGET_DIR','CARGO_BUILD_JOBS','CARGO_PROFILE_DEV_DEBUG','CARGO_PROFILE_TEST_DEBUG','CARGO_TERM_COLOR']}}
(run/'invocation.json').write_text(json.dumps(record,indent=2)+'\n')
with (run/'stdout.log').open('xb') as stdout,(run/'stderr.log').open('xb') as stderr,(run/'combined.log').open('xb') as combined:
 proc=subprocess.Popen(command,cwd=record['cwd'],env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
 sel=selectors.DefaultSelector()
 sel.register(proc.stdout,selectors.EVENT_READ,stdout)
 sel.register(proc.stderr,selectors.EVENT_READ,stderr)
 while sel.get_map():
  for key,_ in sel.select(timeout=1):
   data=os.read(key.fileobj.fileno(),65536)
   if not data:
    sel.unregister(key.fileobj)
    continue
   key.data.write(data); key.data.flush()
   combined.write(data); combined.flush()
   if key.fileobj is proc.stderr:
    sys.stdout.buffer.write(data); sys.stdout.buffer.flush()
 code=proc.wait()
record.update(exit_code=code,finished_utc=datetime.datetime.now(datetime.timezone.utc).isoformat())
(run/'exit-code.txt').write_text(str(code)+'\n')
(run/'result.json').write_text(json.dumps(record,indent=2)+'\n')
print('C2D focused cargo exit code:',code,flush=True)
for line in (run/'stdout.log').read_text().splitlines():
 if line.startswith('C2D ') or line.startswith('test result:'):
  print(line,flush=True)
sys.exit(code)
