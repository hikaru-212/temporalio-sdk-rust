# CCPO C-4 Research Checkpoint

此 checkpoint 封存已完成的 C-4，不執行新實驗。這是 **Temporal Server Go 測試與證據，保存於 SDK-Rust 個人研究 fork**；不能在 SDK-Rust repository 根目錄直接執行本測試 runner。

- Parent：`632c157c02af7154e4b93541c51dc1d17141c003`。
- Pinned Server：`0d5e3848bed625907987f73d5eaae8dc77bc6590`。
- 分支：`research/ccpo-egts-c4-range-barrier`；repository：`hikaru-212/temporalio-sdk-rust`。
- 完整繁體中文歷史報告：[REPORT.zh-TW.md](REPORT.zh-TW.md)。其中「沒有 commit/push」描述的是實驗完成時，不是本次 checkpoint；保存時沒有修改其結論，只替換公開副本的機器路徑。
- 本次公開與完整性稽核：[CHECKPOINT-AUDIT.zh-TW.md](CHECKPOINT-AUDIT.zh-TW.md)。

## 保留結果與證據範圍

| 記錄 | 原始結果 | Exit |
|---|---|---:|
| run-01-control | INCONCLUSIVE：immediate task range 參數不合法，尚未送 completion | 1 |
| run-02-control | INCONCLUSIVE：scheduled task range 參數不合法，尚未送 completion | 1 |
| run-03-control | controlled SQLite CONFIRMED SAFETY | 0 |
| run-04-committed-unknown | controlled SQLite CONFIRMED SAFETY | 0 |
| run-05-not-committed-unknown | controlled SQLite CONFIRMED SAFETY | 0 |
| lint-check | 工具準備失敗，未完成 lint | 2 |

**不是 lint PASS。** 每個 run 保留執行當時 test source、command/result、stdout/stderr、observations 與 protobuf。三個成功 run 使用相同最終測試版本。

最終 test source 在 `server-test/range_barrier_test.go.txt`，SHA-256：`a6fd789b274808c0a807555795ee3598de935694c64bec6af26202a795f02223`。`test-only.diff` 只新增 Server 的 `tests/ccpo_c4/range_barrier_test.go`。所有歷史測試版本與 patch 均未改變。

本 bundle 保留 88 個原 manifest 證據項目及原 manifest 本身；38 個 protobuf 均逐位元保留。`ORIGINAL-SHA256SUMS` 是原 manifest 的原樣副本。`PUBLICATION.json` 對每個原檔記錄公開路徑、原始／公開 hash、大小與轉換。`SHA256SUMS` 驗證公開 bundle，**不能拿原 manifest 直接驗證已去識別的公開 log**。

原 workspace 與 archive 保留本地不變；archive SHA-256：`494c747abb56db1ae788b915021953f8372ec339754170b382bc4dff42a83aba`。公開副本只替換本機 checkout/module-cache 路徑、暫存 SQLite 檔名、log 中原本已遮蔽的 password 設定值。沒有刪除 observation 行、修改時序、protocol identity、task/event ID、結果或模型結論。synthetic task tokens 來自已 teardown 的本機 test cluster，保留以驗證 byte-identical retry 與 protobuf 因果引用；它們不是 GitHub／服務帳號憑證。

## 完整性檢查（不執行實驗）

```bash
cd research-evidence/c4-range-barrier
sha256sum -c SHA256SUMS
python3 verify-public.py
python3 audit-observations.py
```

Verifier 僅讀取此 bundle，核對公開與原始 hash mapping、預登錄、歷史 run exits、log/observation 一致性、所有 protobuf artifacts 與 patch/source；不啟動 Server、Go compiler 或測試。`audit-observations.py` 對既有成功 run 的六類別完整分頁列舉做離線重算，不是新增 persistence 實驗。

## 對 pinned Temporal Server 獨立重現

前置需求：Git、Python3、Go/toolchain 與 dependencies 匹配 pinned Server 的 go.mod、可寫 cache、loopback ports 與磁碟空間。原執行為 Go 1.27.0 / linux arm64，file SQLite WAL / synchronous=normal；詳細來源見 `source-provenance.json`。不需要 Rust build、外部 Temporal cluster、C-3 worktree、原臨時 evidence 或未追蹤 fixture。原 SQLite DB 由正常 testcore teardown 移除；本 bundle 不是 crash database image。

以下僅為未來經授權的重現說明，**本次 checkpoint 沒有執行這些 Go 測試**。從 SDK-Rust checkpoint 根目錄設定來源；目的地必須是新的 Server 目錄。每一步成功後才進行下一步。

```bash
CCPO_BUNDLE="$PWD/research-evidence/c4-range-barrier"
CCPO_SERVER=/tmp/ccpo-c4-server-reproduction

test ! -e "$CCPO_SERVER"
git clone --no-checkout https://github.com/temporalio/temporal.git "$CCPO_SERVER"
git -C "$CCPO_SERVER" checkout --detach 0d5e3848bed625907987f73d5eaae8dc77bc6590
git -C "$CCPO_SERVER" apply --check "$CCPO_BUNDLE/test-only.diff"
git -C "$CCPO_SERVER" apply "$CCPO_BUNDLE/test-only.diff"
cmp "$CCPO_SERVER/tests/ccpo_c4/range_barrier_test.go" "$CCPO_BUNDLE/server-test/range_barrier_test.go.txt"
mkdir -p "$CCPO_SERVER/research-evidence"
cp -R "$CCPO_BUNDLE" "$CCPO_SERVER/research-evidence/c4-range-barrier"
cd "$CCPO_SERVER"
```

先 Control，通過後才 A/B；每次選新的輸出目錄，runner 禁止覆寫：

```bash
python3 research-evidence/c4-range-barrier/run-focused.py control reproduce-01-control
python3 research-evidence/c4-range-barrier/run-focused.py a reproduce-02-a
python3 research-evidence/c4-range-barrier/run-focused.py b reproduce-03-b
```

Runner 必須使用已複製到 **Server checkout** 的副本，才能正確推導 repository root。它設定 `GOMAXPROCS=2` 與 per-run `CCPO_EVIDENCE_OUTPUT`，執行對應的 `go test -p 2 -tags test_dep -count=1 -timeout 3m -run '^TEST_NAME$' -v ./tests/ccpo_c4`，保存原始 streams、exit、protobuf 與 test hash。UUID/timestamp 由 Server 產生，跨重新執行的 fixture hash 不會固定；每個 arm 內原請求／重試須 byte-identical。

## 不可擴張的結論

- A 是 **manager-level committed-then-error**：delegate 已成功完成後回 Timeout。
- B 是 **manager-level noncommitted-then-error**：該次 delegate 沒有執行，再回相同 Timeout。
- **實際 SQL transaction 持續在途時的 commit ambiguity／RangeID race 仍 UNKNOWN**；Optional C 未執行。不是物理 database timeout reproduction。
- RangeID 2 是 authority/recovery evidence，**不是原 WFT commit 成功收據**。A/B 都到 range2，仍須依 fresh MS／有效 history 區分 completed 與 pending。
- 六類持久化 task range 的獨立列舉優於 C-3 recorder-ID discovery，但它不是跨類別 atomic snapshot 或所有時間點的全域證明。
- 未證明 general fencing theorem、crash recovery、ownership transfer、其他 backend、外部 Activity exactly-once、完整 Worker observation indistinguishability 或 general Temporal correctness。

不進入 C-5。
