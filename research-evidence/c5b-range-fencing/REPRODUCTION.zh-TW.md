C-5B.2重現說明（文件；準備checkpoint時未重跑）

本研究在SDK-Rust fork保存，實際執行必須使用獨立的Temporal Server checkout，固定HEAD `0d5e3848bed625907987f73d5eaae8dc77bc6590`。不要在SDK-Rust repository執行Go test，不要覆蓋現有研究workspace。

先備條件：Linux環境，原run為arm64；相容Go1.27.0 executable及pinned go.mod/go.sum所需完整module cache；file-backed SQLite/WAL/private cache可用、至少兩個physical connections、loopback測試服務權限；足夠磁碟及可寫獨立GOCACHE／build-temp。module cache可以readonly，但缺依賴時應停止，不清理共享cache或自行擴大下載。modernc.org/sqlite版本為v1.51.0。只保存source與證據不能讓缺少dependency cache的機器自動離線可跑。

在新乾淨Server checkout中確認HEAD及git status，先檢查本包SHA256SUMS。以完整`patches/test-only.diff`加入三個新檔，或複製`source/tests/ccpo_c5b2/`，二者擇一。差異只新增tests/ccpo_c5b2三檔。`patches/p2-only.diff`供歷史審查，不能再套到目前已修正source。驗證三份source hash與PROVENANCE.json一致。這些步驟不需原先消失的暫存workspace、舊B1 artifacts或private sessions。

下列是可移植模板，不是原始invocation bytes。將路徑變數設成使用者授權的全新持久位置；每個arm用獨立程序和全新輸出目錄。`CCPO_EVIDENCE_OUTPUT`必須事先建立；輸出使用O_EXCL防止覆寫。不要把同一目錄重用三次。

```sh
# 先由操作者設定 SERVER_CHECKOUT、GO_EXECUTABLE、ARM_OUTPUT、
# ISOLATED_GOCACHE、EXISTING_MODULE_CACHE、BUILD_TEMP；ARM為Main/Positive/Negative其中之一。
cd "$SERVER_CHECKOUT"
mkdir -p "$ARM_OUTPUT/runtime" "$ARM_OUTPUT/protobuf" "$ISOLATED_GOCACHE" "$BUILD_TEMP"
env GOTOOLCHAIN=local GOPROXY=off GOSUMDB=off \
  GOCACHE="$ISOLATED_GOCACHE" GOMODCACHE="$EXISTING_MODULE_CACHE" \
  GOTMPDIR="$BUILD_TEMP" TMPDIR="$ARM_OUTPUT/runtime" \
  CGO_ENABLED=0 GOMAXPROCS=2 GOFLAGS=-mod=readonly \
  CCPO_EVIDENCE_OUTPUT="$ARM_OUTPUT/protobuf" \
  TEMPORAL_TEST_LOG_FILE="$ARM_OUTPUT/server.jsonl" \
  timeout --signal=TERM --kill-after=30s 30m "$GO_EXECUTABLE" \
  test -p 2 -mod=readonly -tags test_dep -count=1 -timeout 4m \
  -run "^TestCCPOC5B2${ARM}$" -v ./tests/ccpo_c5b2 \
  >"$ARM_OUTPUT/stdout.log" 2>"$ARM_OUTPUT/stderr.log"
run_exit=$?
printf '%s\n' "$run_exit" >"$ARM_OUTPUT/exit-code.txt"
```

重現時須另記精確executable、environment白名單、source hashes、開始／結束與exit；若shell採set -e，需讓操作者正確擷取失敗exit而非提早結束。不要只憑PASS文字判定；依METHODOLOGY核對SQL真實順序、guarded read、前後State/history／六類task與cleanup，任一必要證據缺漏為INCONCLUSIVE。此模板並未在新公開bundle下執行，所以只聲稱source與原PASS一致、依賴已文件化，不聲稱新的重現run成功。

只想稽核本checkpoint時，執行 `python3 tools/verify_public.py` 與 `sha256sum -c SHA256SUMS`即可；它們只讀已保存的public files，不編譯、不啟動Temporal、不產生SQL transaction。
