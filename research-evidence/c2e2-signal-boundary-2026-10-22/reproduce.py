"""Reproduce only the archived C2-E2 focused tests, into a new output directory."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--verify-only', action='store_true')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--build-dir', type=Path)
    parser.add_argument('--allow-network', action='store_true')
    args = parser.parse_args()
    evidence = Path(__file__).resolve().parent
    root = evidence.parent.parent
    manifest = json.loads((evidence / 'SHA256SUMS.json').read_text())
    for relative, expected in manifest.items():
        if digest(evidence / relative) != expected:
            raise SystemExit('Evidence hash mismatch: ' + relative)
    provenance = json.loads((evidence / 'checkpoint-provenance.json').read_text())
    for relative, expected in provenance['test_source_sha256'].items():
        if digest(root / relative) != expected:
            raise SystemExit('Test source hash mismatch: ' + relative)
    fixture = evidence / 'inputs/E2-D.pb'
    if digest(fixture) != '68de8fb53e38036a3cf155ce9d63169e0bad4d371ca4fbc2a95df4d060a7e1df':
        raise SystemExit('Original E2-D input mismatch')
    lock_source = evidence / 'dependency.Cargo.lock'
    lock = root / 'Cargo.lock'
    if lock.exists() and digest(lock) != digest(lock_source):
        raise SystemExit('Existing Cargo.lock differs; preserve it and use a separate checkout')
    print(json.dumps({'verified_evidence_files': len(manifest),
                      'verified_test_sources': len(provenance['test_source_sha256']),
                      'input_sha256': digest(fixture),
                      'cargo_lock_present': lock.exists()}), flush=True)
    if args.verify_only:
        return 0
    if args.output is None:
        parser.error('--output is required unless --verify-only is selected')
    output = args.output.resolve()
    if output.exists() or output == evidence or evidence in output.parents:
        raise SystemExit('Use a new output directory outside archived evidence')
    output.mkdir(parents=True)
    if not lock.exists():
        with lock.open('xb') as target:
            target.write(lock_source.read_bytes())
    build = args.build_dir.resolve() if args.build_dir else output / 'build'
    for stage in ('scanner', 'H0', 'Hs'):
        run = output / stage
        run.mkdir()
        fixtures = run / 'fixtures'
        fixtures.mkdir()
        test = ('scanner_preserves_signal_boundary' if stage == 'scanner'
                else 'warm_cold_signal_compatibility')
        command = ['cargo', 'test', '--locked']
        if not args.allow_network:
            command.append('--offline')
        command += ['-p', 'temporalio-sdk-core', '--lib',
                    'worker::workflow::managed_run::c2e2_signal_boundary::' + test,
                    '--', '--exact', '--nocapture']
        overrides = {
            'TEMPORAL_USE_WFT_CHUNKING_V2': '1',
            'C2E2_INPUT': str(fixture), 'C2E2_OUTPUT_DIR': str(fixtures),
            'C2E2_FAMILY': stage, 'CARGO_TARGET_DIR': str(build),
            'CARGO_BUILD_JOBS': '2', 'CARGO_PROFILE_DEV_DEBUG': '0',
            'CARGO_PROFILE_TEST_DEBUG': '0', 'CARGO_TERM_COLOR': 'never',
        }
        invocation = {
            'stage': stage, 'command': command, 'cwd': str(root),
            'environment_overrides': overrides,
            'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
            'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
            'production_base_sha': provenance['production_base_sha'],
            'test_source_sha256': provenance['test_source_sha256'],
            'predictions_public_sha256': digest(evidence / 'predictions.before-run.json'),
            'dependency_lock_sha256': digest(lock),
        }
        (run / 'invocation.json').write_text(json.dumps(invocation, indent=2) + '\n')
        shutil.copyfile(evidence / 'test-only.diff', run / 'test-only.diff')
        for relative in provenance['test_source_sha256']:
            source = root / relative
            shutil.copyfile(source, run / source.name)
        print('RUN ' + stage + ': ' + ' '.join(command), flush=True)
        with (run / 'stdout.log').open('wb') as stdout, (run / 'stderr.log').open('wb') as stderr:
            result = subprocess.run(command, cwd=root, env={**os.environ, **overrides},
                                    stdout=stdout, stderr=stderr)
        (run / 'exit-code.txt').write_text(str(result.returncode) + '\n')
        (run / 'finished-utc.txt').write_text(datetime.datetime.now(datetime.timezone.utc).isoformat() + '\n')
        hashes = {str(p.relative_to(run)): digest(p) for p in sorted(run.rglob('*')) if p.is_file()}
        (run / 'SHA256SUMS.json').write_text(json.dumps(hashes, indent=2) + '\n')
        print('EXIT ' + stage + ': ' + str(result.returncode), flush=True)
        if result.returncode:
            print('Stopped before the next stage. Classify compiler/fixture/adapter failure separately from product correctness.', flush=True)
            return result.returncode if result.returncode > 0 else 1
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
