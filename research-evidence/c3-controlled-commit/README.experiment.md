# CCPO C-3 controlled normal WFT completion

完整繁體中文結果見 REPORT.zh-TW.md；機器可讀摘要見 RESULTS.json。所有結論限真實 onebox Server + file SQLite 的 normal、already-started WFT，不是外部 Activity exactly-once。

## Reproduction

需要 Server source `0d5e3848bed625907987f73d5eaae8dc77bc6590`、Go 1.27.0（本次 Linux arm64）、Python3、可寫 Go module/build cache、可使用 loopback ports 與 temporary SQLite files。首次需有該來源版本的 Go dependencies。先檢查磁碟；本次開始約 21–22 GiB 可用，重用既有 cache 並以 `-p 2` 限制平行編譯。不需要外部 Server、Docker、SQL daemon、Rust SDK 或舊 research fixture。

在新的 clean checkout 套用 `test-only.diff`；不要在含既有研究變更的 checkout 套用。將本 evidence 目錄複製至 repository 的 `research-evidence/c3-controlled-commit/`，runner 會從自身位置推得 repository root，不依賴原 <TEMP_ROOT> 路徑。既有 run 目錄禁止覆寫；請使用新名稱。

```bash
python3 research-evidence/c3-controlled-commit/run-focused.py control reproduce-control
python3 research-evidence/c3-controlled-commit/run-focused.py a reproduce-a
python3 research-evidence/c3-controlled-commit/run-focused.py b reproduce-b
```

只有 Control/A/B 全部成功後才選擇執行：

```bash
python3 research-evidence/c3-controlled-commit/run-focused.py concurrent reproduce-concurrent
```

每次只跑一個指定 test。Runner 會使用 GOMAXPROCS=2，保存 argv／test SHA、stdout、stderr、exit code，且遇到已存在的 run 目錄直接停止。預登錄位於 PREDICTIONS.md；不可拿測試對預測的失敗改寫成 expected fatal。

## Evidence

- run-01～04：原始四個 arm；harness-v1.test.go.txt／harness-v1.diff 保存原測試。
- run-05～08：同一實驗，只增加所有成功 task-write 類型記錄；最終測試為 tests/ccpo_c3/commit_ambiguity_test.go，test-only.diff 可重建。
- 原始 stdout.log／stderr.log 未變更；observations.json 只是逐行擷取 `CCPO {…}`，附來源 hash 與原行號。
- 所有直接讀取均在測試存活期間對真實 persistence 執行；MS 的 NextEventId 限定有效 history。Activity transfer row 另獨立讀取並經真實 poll 證明可派送。
- SQLite ephemeral test DB 在正常 teardown 清除；沒有 DB image。未知 backend/crash/外部 effect 行為不得從本例推導。
- lint-check 保存 repository lint target 無法在 cached-only/offline 環境準備工具的原輸出（exit2）；runtime test／compile 全部 exit0。
- 本地原始 log 有 <TEMP_ROOT> 路徑與測試 UUID；所有業務 identity 都是合成的。未把 C-3 公開、commit 或 push。

從本目錄執行 `sha256sum -c SHA256SUMS`。Manifest 包含最終測試的相對路徑，故應保持上述 repository 位置；manifest 本身的 hash 記在外部 checkpoint verification output。
