#!/usr/bin/env python3
"""Verify the checkpoint; run only C2-D when an explicit fresh output path is supplied."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

EVIDENCE = Path(__file__).resolve().parent
REPOSITORY = EVIDENCE.parents[1]
BASE = "8f4a676f9e1a6276b5cc2f77e25c9183ce256fe9"
SOURCE = "crates/sdk-core/src/worker/workflow/managed_run.rs"
TEST = "worker::workflow::managed_run::c2d_live_replay::live_replay_semantic_convergence"


def verify():
    manifest = json.loads((EVIDENCE / "SHA256SUMS.json").read_text())
    for name, expected in manifest.items():
        actual = hashlib.sha256((EVIDENCE / name).read_bytes()).hexdigest()
        if actual != expected:
            raise RuntimeError(f"Evidence hash mismatch: {name}")
    original = EVIDENCE / "inputs/C2-B-E2.pb"
    if hashlib.sha256(original.read_bytes()).hexdigest() != "6634b01fac15a734c9fd92807f5bb1b3430201572da9b132b86b1987880a774c":
        raise RuntimeError("Original C2-B E2 input mismatch")
    base_source = subprocess.check_output(["git", "show", f"{BASE}:{SOURCE}"], cwd=REPOSITORY)
    if (REPOSITORY / SOURCE).read_bytes() != base_source + (EVIDENCE / "c2d_module.rs").read_bytes():
        raise RuntimeError("Test source is not the exact pinned source plus the recorded C2-D module")
    print(f"Verified {len(manifest)} evidence files, original E2 input, and exact C2-D test source.")
    return original


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verify-only", action="store_true", help="Integrity checks only; no Cargo, compilation or tests")
    parser.add_argument("--output", type=Path, help="New directory outside the archived evidence")
    parser.add_argument("--allow-network", action="store_true", help="Omit Cargo --offline if locked dependencies are not cached")
    args = parser.parse_args()
    original = verify()
    if args.verify_only:
        return 0
    if args.output is None:
        parser.error("Supply --output with a new directory to execute the focused test")
    output = args.output.expanduser().resolve()
    if output == EVIDENCE or EVIDENCE in output.parents:
        parser.error("Output must be outside archived evidence")
    output.mkdir(parents=True, exist_ok=False)
    fixtures = output / "fixtures"
    fixtures.mkdir()
    overrides = {
        "C2D_OUTPUT_DIR": str(fixtures),
        "C2D_ORIGINAL_E2": str(original),
        "TEMPORAL_USE_WFT_CHUNKING_V2": "1",
        "CARGO_TARGET_DIR": os.environ.get("C2D_BUILD_DIR", str(output / "build")),
        "CARGO_BUILD_JOBS": "2",
        "CARGO_PROFILE_DEV_DEBUG": "0",
        "CARGO_PROFILE_TEST_DEBUG": "0",
        "CARGO_TERM_COLOR": "never",
    }
    command = ["cargo", "test", "--locked"]
    if not args.allow_network:
        command.append("--offline")
    command += ["-p", "temporalio-sdk-core", "--lib", TEST, "--", "--exact", "--nocapture"]
    record = {
        "command": command, "cwd": str(REPOSITORY), "source_base": BASE,
        "environment_overrides": overrides,
        "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    }
    (output / "invocation.json").write_text(json.dumps(record, indent=2) + "\n")
    print("Executing only:", " ".join(command), flush=True)
    try:
        with (output / "stdout.log").open("xb") as stdout, (output / "stderr.log").open("xb") as stderr:
            result = subprocess.run(command, cwd=REPOSITORY, env={**os.environ, **overrides}, stdout=stdout, stderr=stderr)
        code = result.returncode
    except OSError as error:
        code = 127
        record["harness_error"] = str(error)
    record.update(exit_code=code, finished_utc=datetime.datetime.now(datetime.timezone.utc).isoformat())
    (output / "exit-code.txt").write_text(str(code) + "\n")
    (output / "result.json").write_text(json.dumps(record, indent=2) + "\n")
    hashes = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in fixtures.glob("*.pb")}
    (output / "fixture-sha256.json").write_text(json.dumps(hashes, indent=2) + "\n")
    print(f"Exit {code}; full stdout/stderr and fixture hashes saved in {output}")
    return code


if __name__ == "__main__":
    sys.exit(main())
