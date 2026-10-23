# C-4 Checkpoint 稽核

本文件是 checkpoint 保存作業的記錄，不是新增 runtime 實驗。原研究報告及結論僅做公開路徑替換，不回溯改寫。

## 原始完整性與 Git 隔離

原 Server experiment 是 detached `0d5e3848bed625907987f73d5eaae8dc77bc6590`；沒有 tracked diff，只有 C-4 evidence directory 與新增 test。88 個 manifest 項目逐一通過；原 manifest 檔案 hash `7a86d208a336d191a2b4b457a2ca81da516949233d9134b200844220892973c5`。

原 tar archive hash `494c747abb56db1ae788b915021953f8372ec339754170b382bc4dff42a83aba`；archive 的 90 個 regular files（89 個 evidence files 加一個 Server test）逐位元等同 workspace。最終測試 hash 與三個成功 run 相符。保存時沒有重新執行 Go test 或 compilation。

獨立 SDK-Rust clone 從精確 parent `632c157c02af7154e4b93541c51dc1d17141c003` 建立新分支，避免改動原 checkpoint 的分支、index 或未提交工作。只提交 `research-evidence/c4-range-barrier/`；Go source 放在 evidence 中，不改 Rust 或 Server production implementation。

## 公開資料稽核與轉換

原始檔案與 archive 保留本地。`PUBLICATION.json` 提供 89 個原檔的一對一 hash mapping；`publication-transform.py` 定義可重現的公開轉換，拒絕覆寫既有檔案。

1. 將本機 Server／C-3 checkout、個人 home 下 module cache 路徑改為具名 placeholder；保留 source-relative file、函數、行號、stack 與事件順序。
2. 將 ephemeral SQLite DB 檔名改為 placeholder。
3. 五份 stdout 的 YAML `password` 設定在原始輸出已是 `******`；公開副本統一為 `<REDACTED_TEST_PASSWORD>`。`passwordCommand: null` 與空 authorization 設定不是憑證。
4. 38 個 protobuf 不做任何轉換。五組 WFT tokens 均可與其 synthetic namespace/workflow/run 的 request observation 對應；recovery Activity token 也屬同一個本機測試 execution。沒有 GitHub access token、Bearer credential、private key 或非 loopback IP。這些 test tokens 是協定研究資料，不是第三方服務 access token。
5. 保留 stack 中 temporary pointer values，以維持同一 shard ContextImpl 的對應證據；不將其當成長期 identity。保留 timestamps、UUIDs、RangeIDs、DB revisions 與 event/task IDs，避免破壞因果證據。
6. 最大單檔約 146 KB，整體不足 1 MB；沒有 build cache、target、整個 Server checkout、database image、環境變數 dump 或其他 worktree 的內容。

共 21 個來源檔的公開 bytes 改變，68 個原檔 bytes 不變；所有變更都只屬上述轉換。`README.experiment.md` 保留原 README 的公開副本；新 `README.md` 另述 SDK-Rust fork → pinned Server 的正確重現流程。原 manifest 改名為 `ORIGINAL-SHA256SUMS`，內容不變；新 manifest 核對公開 bytes。

## 證據含義與重現審核

- 兩次 harness INCONCLUSIVE、三次 controlled SQLite PASS 與 lint preparation exit 2 均保留。**沒有 lint PASS**。
- `audit-observations.py` 重算既有 logs：A 的 MS/task 集合在 retry 前後不變；B barrier 後仍與初始 pending state 相同，之後只接受一次。六類 task 都以 full range/page token 枚舉，不使用 recorder IDs。
- 已核對 patch 的新增內容與 final Go source 相同；對乾淨 pinned Server 做 `git apply --check`。這是 patch／dependency review，不是重跑 runtime tests。
- runner 與 Go source 不依賴 C-3 臨時目錄、未追蹤 fixture 或本機 evidence；Server 產生每次 history，再保存 protobuf。需將 runner 複製進 Server checkout，不能從 SDK-Rust fork 直接跑。
- A/B 分別是 manager-level committed/noncommitted-then-error 模擬，確實經過 uncertain-write handler 與真實 range renewal；**不是 SQL in-flight commit timeout 重現**。
- RangeID 2 證明 recovery authority 更新，不決定原 WFT 是否提交。有效 MS/history 與 barrier/freshness 契約必須另行檢查。
- Optional C、crash、ownership transfer、其他 backend、external effects、general fencing theorem 都仍未證明。沒有開始 C-5。

遠端 repository owner、登入帳號、push URL、branch 狀態及 parent 已在提交前驗證；紀錄見 `CHECKPOINT-AUDIT.json`。push 完成後的 exact local/remote SHA 另由 checkpoint receipt 與對話結尾提供，避免為了把自身 commit SHA 寫入 commit 而改寫歷史。

`git diff --cached --check` 對五份原始 stdout log 的縮排／空白提出 141 個 whitespace 訊息；不為消除提示而修改證據 bytes。新增文件／檢查腳本沒有 whitespace 提示。分類與 exit code 保存在 CHECKPOINT-AUDIT.json。
