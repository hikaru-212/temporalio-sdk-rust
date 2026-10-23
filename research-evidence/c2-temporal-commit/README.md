# CCPO C-2：Temporal Durable Commit Boundary

本目錄保存已完成的 C-2 唯讀來源調查。原始報告存在於 Codex session 的 assistant 最終訊息中，共 256 行；並非先前獨立的磁碟報告。本次直接從該訊息紀錄擷取完整文字，保存為註明來源的對話重建文件，不憑記憶重寫。

`REPORT.reconstructed.zh-TW.md` 保留原回答全部內容，包括前段 C-1 保存紀錄。唯一公開轉換是把本地來源連結改成固定 SHA 的 GitHub permalink；文字、結論、表格、行數及換行慣例不變。未轉換的原始擷取稿另保存在本機 archive，其 SHA-256、訊息時間與來源紀錄位置見 `provenance.json`。`publication-links.json` 逐項列出原連結雜湊及轉換後的來源定位；沒有公開整份 session log 或其他對話。

## 固定來源

- Server：`temporalio/temporal@0d5e3848bed625907987f73d5eaae8dc77bc6590`
- SDK-Rust：`temporalio/sdk-rust@8f4a676f9e1a6276b5cc2f77e25c9183ce256fe9`
- 父 checkpoint：`029a790e4fa7663cf70c8fedc82b379d47f99dbb`
- 目標：`hikaru-212/temporalio-sdk-rust`，`research/ccpo-egts-c2-temporal-commit`

原調查讀取乾淨的 Server checkout，並核對引用的 SDK production files 與 pinned SHA 一致。公開稿各來源連結再次以 Git object 驗證檔案存在及行號有效。這是來源定位核對，不是重新執行原實驗。

## 內容索引

| 內容 | 原報告位置 |
|---|---|
| 版本、normal WFT scope、speculative/paginated exceptions | 第 2 節 |
| RPC call chain、history append、SQL/Cassandra commit boundaries | 第 3 節 |
| Activation、commit、ack 的區分；task identity、attempt、ownership | 第 4 節 |
| Retry、eviction、duplicate safeguards、RangeID、unknown write outcome | 第 5 節 |
| 候選不可區分世界與觀察函數 | 第 6 節 |
| Safe recovery、history evidence、限定安全命題 | 第 7 節 |
| 外部 effects、假設與未知義務 | 第 8 節 |
| CCPO 適用範圍與研究分類 | 第 9 節 |

## 證據與限制

Source-confirmed 是讀取固定來源所得；Model-inferred 是附條件的推論；本調查沒有新增 Runtime-supported 證據。不把既有 C2-D/C2-E2 PASS 轉作 commit ambiguity runtime 證據，不宣稱已證明外部 exactly-once、一般 semantic refinement 或 Temporal correctness bug。

本 checkpoint 只有文件與完整性紀錄。C-3 實驗尚未開始，其後的結果不得回填修改本次原始結論。

從此目錄執行 `sha256sum -c SHA256SUMS` 可核對新增文件。Manifest 不包含自身，交由 Git commit 保護。核對文件不需要 build cache、Server、資料庫或舊 temporary worktree。原文引用的外部 gRPC 文件僅保存連結，不複製全文。
