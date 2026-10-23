# CCPO C-3 research checkpoint

本 checkpoint 保存已完成的四個 logical scenarios 與八次 focused run，不新增實驗。這是 **Temporal Server 的 Go 測試，封存於 SDK-Rust 個人研究 fork**；不能在 SDK-Rust repository 根目錄直接執行本測試 runner。

- Parent checkpoint：`70acd8a9eaad6a14a0b58ccf113c0cfda9e4cad1`。
- Server source：`0d5e3848bed625907987f73d5eaae8dc77bc6590`。
- 目標分支：`research/ccpo-egts-c3-commit-injection`，個人 repository `hikaru-212/temporalio-sdk-rust`。
- 完整歷史報告：[REPORT.zh-TW.md](REPORT.zh-TW.md)。報告內「未 commit／push」描述的是實驗完成當時的狀態；本次保存不回改其結論或文字。
- 本次獨立審核：[AUDIT.zh-TW.md](AUDIT.zh-TW.md)。**persisted-task 查驗依賴 recorder 發現 IDs，並非完整 task range 的獨立列舉**。不可把原始 PASS 擴張為未觀測 task 不存在的獨立證明。

## 保存內容與證據來源

| 情境 | 初版 run | 補齊 task ledger 的 run | 原始 exit codes |
|---|---|---|---|
| 正常完成 Control | run-01-control | run-05-control-ledger | 0 / 0 |
| 提交成功後抑制成功 response | run-02-response-lost | run-06-response-lost-ledger | 0 / 0 |
| precommit failure 後原請求重試 | run-03-precommit | run-07-precommit-ledger | 0 / 0 |
| 同一 WFT identity 的並行完成 | run-04-concurrent | run-08-concurrent-ledger | 0 / 0 |

每組保留 command.json、result.json、stdout.log、stderr.log、observations.json。預登錄、兩版測試及其差異、fault definitions、完整 MS/history/task observations 均保留。`lint-check/` 保存工具準備失敗 exit 2，**不是 lint PASS**。

`server-test/commit_ambiguity_test.go` 是最終測試的逐位元相同副本；SHA-256 `22b5bf7cd197edfaa78452452e1f239e8d7d4997aceefc9301b95b79ca213f25`。`test-only.diff` 應套用到 Server repository 的 `tests/ccpo_c3/commit_ambiguity_test.go`。`harness-v1.test.go.txt`／`harness-v1.diff` 保存 run-01～04 的版本，SHA-256 `67a4041a30e989ff8565ef3c85a6e8120816fd15ccaa882ba3119f363f95ebe9`。

原始 evidence directory、Server experiment checkout 與 tar archive 均保留本地原樣。公開副本只去除機器路徑、暫存 DB 檔名／裝置標籤、測試環境 password 欄位與無關 worktree 檔案清單，沒有更改 event/task identity、時序、狀態、退出碼或結果。`PUBLICATION.json` 對每個來源檔保存原始／公開 SHA-256、大小與轉換。原封存 SHA-256：`30939a09296226c6a065e9d88513ae033bd4311c4c6556b13aad70c1be284439`。

`ORIGINAL-SHA256SUMS` 是原 manifest 的原樣副本；其中相對路徑對應原 Server workspace。`observations.json.stdout_sha256` 也保留對**原始 stdout**的引用，請由 PUBLICATION.json 對應公開副本 hash，不要誤認公開 log 未經路徑遮蔽。公開副本仍保留 stdout 原行號與全部 CCPO observation 值。

## 完整性核對（不執行實驗）

在此目錄執行：

```bash
sha256sum -c SHA256SUMS
python3 verify-public.py
```

SHA256SUMS 只涵蓋本 evidence directory 的檔案，不依賴原 /tmp checkout。Verifier 核對原／公開 hash 關係、兩版 patch/source、八次結果及擷取 observation 的原行號；不啟動 Go、Server 或測試。

## 對 pinned Server 重現

需求：Go 1.27.0、Python3、Git、該 Server 版本的 Go dependencies、可寫 module/build cache、loopback ports、暫存檔空間。原環境為 Linux arm64；SQLite 為 file-backed WAL / synchronous=normal。不需要外部資料庫服務、Rust SDK、原暫存 workspace、歷史 DB image 或其他研究 fixture。開始前檢查可用磁碟；原執行約有 21–22 GiB 可用並重用既有 cache。

以下命令僅供之後經授權的重現，不是本 checkpoint 的新執行紀錄。從 SDK-Rust checkpoint 根目錄設定 bundle 路徑，使用**不存在的新 Server 目錄**：

```bash
CCPO_BUNDLE="$PWD/research-evidence/c3-controlled-commit"
CCPO_SERVER=/tmp/ccpo-c3-server-reproduction

test ! -e "$CCPO_SERVER"
git clone --no-checkout https://github.com/temporalio/temporal.git "$CCPO_SERVER"
git -C "$CCPO_SERVER" checkout --detach 0d5e3848bed625907987f73d5eaae8dc77bc6590
git -C "$CCPO_SERVER" apply --check "$CCPO_BUNDLE/test-only.diff"
git -C "$CCPO_SERVER" apply "$CCPO_BUNDLE/test-only.diff"
cmp "$CCPO_SERVER/tests/ccpo_c3/commit_ambiguity_test.go" "$CCPO_BUNDLE/server-test/commit_ambiguity_test.go"
mkdir -p "$CCPO_SERVER/research-evidence"
cp -R "$CCPO_BUNDLE" "$CCPO_SERVER/research-evidence/c3-controlled-commit"
cd "$CCPO_SERVER"
```

每一步成功後再執行下一步，避免套用到既有研究 checkout。確認 HEAD 正確後，依序只跑下列聚焦 arm：

```bash
python3 research-evidence/c3-controlled-commit/run-focused.py control reproduce-control
python3 research-evidence/c3-controlled-commit/run-focused.py a reproduce-a
python3 research-evidence/c3-controlled-commit/run-focused.py b reproduce-b
```

三者全部通過才可選擇執行 `python3 research-evidence/c3-controlled-commit/run-focused.py concurrent reproduce-concurrent`。Runner 禁止覆寫同名 run 目錄；它從**已複製到 Server 的自身路徑**推得 repository root。等價 Go 命令、GOMAXPROCS=2 及每次原始 invocation 均已保存，沒有隱藏的本機 fixture 依賴。

## 結論限制

SQLite only；normal WFT only；受控 post-handler response suppression 與受控 pre-store failure。未測 mid-transaction uncertain persistence result、crash recovery、ownership transfer 或外部 Activity exactly-once；沒有證明 A/B 的完整 Worker observation 相同，也沒有一般 Temporal correctness theorem。全範圍、與 recorder discovery 無關的 persistence enumeration 仍是未完成義務。本次不重跑測試、不補做 enumeration、不進入 C-4。
