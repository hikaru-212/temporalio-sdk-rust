# C2-D 研究 checkpoint

研究基線：`8f4a676f9e1a6276b5cc2f77e25c9183ce256fe9`。此分支僅保存研究，沒有 production repair 或 upstream PR。

這是**一個受控 warm/cold 收斂見證，不是一般語意等價證明**。正式 WorkflowManager 與 Rust Guest runtime 執行同一程式；LA executor 是同步、確定的測試替身，注入 runtime=17 秒，沒有真實外部 LA worker 或 wall-clock sleep。Warm/cold 初始化 metadata 不同，包括 replay flag、deployment 和 last SDK version；不能泛稱這些差異不可觀察。真實 Server reachability、failure recovery、任意 workflow 與一般 refinement 均未證明。

## 已保存的結果

- `predictions.before-run.json`：執行前預測，公開副本僅替換本機路徑。
- `REPORT.zh-TW.txt`、`observations.json`、`activation-timeline.json`：結論及限制。
- `run-01/`：exit **101**，builder 重複設定 task queue，編譯／harness **INCONCLUSIVE**，未產生產品 RED；失敗版本及 diff 保留。
- `run-02/`：exit **0**，focused test **PASS**；原始 stdout/stderr、命令、fixture 與 activation protobuf 保留。
- `c2d_module.rs`、`test-only.diff`：實際通過的測試，與 source 中追加的 `cfg(test)` 模組逐 byte 相同。保留執行時原樣，未為 checkpoint 重新格式化。
- `inputs/C2-B-E2.pb`：原始 C2-B E2 的精確副本，解決舊 runner 對另一 worktree 的依賴；不需重跑 C2-B。

| Fixture | SHA-256 |
|---|---|
| 原始 C2-B E2 | `6634b01fac15a734c9fd92807f5bb1b3430201572da9b132b86b1987880a774c` |
| 本次 E2-D | `68de8fb53e38036a3cf155ce9d63169e0bad4d371ca4fbc2a95df4d060a7e1df` |

**E2-D 與原始 E2 是分開的 fixture。** 本次 A/B 共用同一份 E2-D，並非比較 E1 與 E2。E2-D 保存 warm Core 實際 metadata／marker；`original_schedule_time` 來自 worker clock，與虛擬 workflow time 是不同時鐘域。重跑時該欄位及 protobuf map／flag 排序可能不同，所以新 E2-D 的 byte hash 不保證等於歷史樣本；每次 A/B 仍須共用當次相同 bytes，且固定語意 assertions 不變。

## 重現

前置條件：此 research branch 的完整 checkout、Git、Python 3.9+、`rust-toolchain.toml` 指定的 Rust 1.94（原始執行 1.94.1）、Cargo、Linux 系統 compiler/linker 與 repository README 要求的 `protoc`。需有 `Cargo.lock` 對應依賴或允許下載，並預留數 GB 的建置空間。無需 Server、GitHub credentials 或舊 worktree／build cache。

在 repository root，先做純靜態檢核：

```bash
python3 research-evidence/c2d-live-replay-2026-10-22/reproduce.py --verify-only
```

需要執行原 focused regression 時，指定**尚不存在**的新輸出目錄：

```bash
python3 research-evidence/c2d-live-replay-2026-10-22/reproduce.py \
  --output /tmp/c2d-reproduction-NEW
```

預設 `--locked --offline`；若 dependency cache 不足，可加 `--allow-network`，仍使用 lockfile。選用 `C2D_BUILD_DIR` 指定獨立 Cargo target cache。預設 build、stdout/stderr、exit code、實際命令及 fixture hashes 全部留在新輸出目錄；wrapper 拒絕覆寫 archived evidence。

測試核心命令與原始 run-02 相同：

```text
cargo test --locked --offline -p temporalio-sdk-core --lib worker::workflow::managed_run::c2d_live_replay::live_replay_semantic_convergence -- --exact --nocapture
```

Wrapper 設定 `C2D_ORIGINAL_E2` 為本 checkpoint 的 bundled input，`C2D_OUTPUT_DIR` 為新 fixtures 目錄，並在新 process 啟動前設定 `TEMPORAL_USE_WFT_CHUNKING_V2=1`。必須保留 `--exact`，不要因此執行整個 test suite。獨立 checkout 沒有先前其他研究測試，filtered-out 數量可與歷史 log 不同。

本輪 checkpoint **沒有重新編譯或執行測試**；只驗證檔案雜湊、來源組合與 wrapper 靜態完整性。因此 portable wrapper 的首次完整重跑尚未執行，不冒稱新環境 runtime PASS。Compiler、依賴或 harness 失敗須先分類 INCONCLUSIVE。

## 完整性與公開副本

原始本機 C2-D、C2-B、其他研究 worktree／分支／索引均保持不變。`original-SHA256SUMS.json` 保存原始證據雜湊；`publication-map.json` 逐檔記錄公開副本是否精確、僅路徑替換或省略，以及對應 hashes。`SHA256SUMS.json` 驗證本公開套件。

公開文字中的本機絕對路徑使用 `__C2D_SOURCE_WORKSPACE__` 等 placeholders；沒有更動觀察值、命令 flags、結果或 protobuf。歷史 `run-focused.py` 及 invocation 紀錄是 provenance，**不要直接執行含 placeholder 的舊 runner**，請用 `reproduce.py`。預登記仍是原始內容的路徑遮蔽副本，沒有事後改寫預測。

未納入無關研究 diff、可由 pinned Git 取回的整份 source snapshot、整個 worktree inventory 或重複 combined logs；原件留在本機。已檢查常見 credential/token/key 格式、本機路徑及檔案大小，未發現 credentials；未納入 build cache 或大型產物。掃描不構成一般性保密保證。
