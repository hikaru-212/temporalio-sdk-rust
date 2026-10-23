# C-4 evidence bundle

Pinned Temporal Server: `0d5e3848bed625907987f73d5eaae8dc77bc6590`.
This local-only bundle contains the Traditional Chinese report, preregistration, exact test-only patch, every focused run (including two harness INCONCLUSIVE attempts), raw stdout/stderr, exit codes, deterministic protobuf observations, independent task enumeration audit, and source provenance. No production change, commit, push, or external Activity execution occurred.

Read `REPORT.zh-TW.md` for evidence labels and limitations. A/B are controlled manager-level outcome simulations that exercise the real shard recovery path, not physical SQL commit-timeout reproductions. Optional in-flight Arm C remains UNKNOWN. Lint preparation failed (exit 2); no lint PASS is claimed.

## Reproduce in a separate pinned Server checkout

Use a new clean Temporal Server checkout at the SHA above. Do not apply this patch to the SDK-Rust fork or an existing research workspace with changes. The patch adds only `tests/ccpo_c4/range_barrier_test.go`.

Prerequisites: Go/toolchain and modules matching the pinned go.mod (observed Go 1.27.0 linux/arm64, exact environment in source-provenance.json), compatible cache or dependency access, available disk, local loopback sockets for testcore onebox, Python 3 for the evidence runner. This experiment uses SQLite and does not require an external Temporal Server. No C-3 temporary file, SDK-Rust checkout, or local fixture is needed: the workflow histories are created by the real test Server and saved by this test.

From the new Server checkout, use the absolute path to this evidence directory as `evidence_bundle`:

```bash
git rev-parse HEAD
git apply --check "$evidence_bundle/test-only.diff"
git apply "$evidence_bundle/test-only.diff"
mkdir -p research-evidence/c4-range-barrier
cp "$evidence_bundle/run-focused.py" research-evidence/c4-range-barrier/
python3 research-evidence/c4-range-barrier/run-focused.py control reproduce-01-control
python3 research-evidence/c4-range-barrier/run-focused.py a reproduce-02-a
python3 research-evidence/c4-range-barrier/run-focused.py b reproduce-03-b
```

Run Control first. Each output directory must be new; the runner refuses existing directories. The wrapper records exact commands, source/test hashes, raw streams, exit codes and protobufs. UUID/timestamp-dependent fixture hashes differ across runs; within each arm the completion request stays byte-identical across retry. The fixed final source hash is in the report and each successful historical run record.

The independent evidence audit of saved original successful runs does not execute Go:

```bash
python3 "$evidence_bundle/audit-observations.py"
(cd "$evidence_bundle" && sha256sum -c SHA256SUMS)
```

The final test source is also preserved as `server-test/range_barrier_test.go.txt`. Every historical run has its exact `test-source.go.txt`. Raw logs are unchanged and contain local paths, generated synthetic task tokens, and test-cluster configuration; they are local research evidence, not a reviewed/redacted public bundle. The temporary SQLite databases were removed by normal testcore teardown; persisted-state observations and protobufs remain, but there is no retained database/crash image.
