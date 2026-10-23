# CCPO Phase C-1：Ambiguous Commit and Indistinguishable States

本目錄保存已完成的 C-1 研究，不包含 Temporal 修復、runtime 實驗或 C-2 結論。

## 來源與保存方式

`REPORT.reconstructed.zh-TW.md` 是依本 Codex 對話中 C-1 最終回答重建的完整繁體中文稿。它不是先前已存在的磁碟報告，也不是平台匯出的原始訊息檔；不宣稱與平台底層序列化資料逐位元相同。保存時保留原有九節、數學定義、表格、推論、限制及引用，不以後續 Temporal 調查修改原結論。

原文的「本輪只有唯讀資料研究」指先前 C-1 分析；本次 checkpoint 另獲授權新增文件、提交與推送。此說明與來源紀錄是本次新增的保存資訊，不是 C-1 原文的一部分。

原本 temporary checkpoint 目錄已不存在，因此本次從已驗證的個人 fork 建立獨立 clone；沒有 reset、stash、clean、改寫舊分支或碰觸既有 worktree 的研究檔案。父 checkpoint 為 `027c13363515d9e4807371777ca510fe00ee7f68`，其父 C2-D 為 `899ccfbfbf59d80797dc0274ff6c20c96e608f65`；production 研究基線為 `8f4a676f9e1a6276b5cc2f77e25c9183ce256fe9`。

## 完整內容索引

| 必要內容 | 報告位置 |
|---|---|
| 狀態轉移、operation/attempt/epoch、durable intent 與外部效果、ack sent/received | 第 1 節 |
| 兩個可達世界及中間世界 | 第 2 節 |
| 完整觀察函數與不可區分證明 | 第 3 節 |
| 受限 safety/liveness 論證、status query 與 late-request race | 第 4 節 |
| 安全重試、idempotency、fencing 與 deduplication、反例 | 第 5 節 |
| Belief-state admission、限定 assume–guarantee 命題與歸納義務 | 第 6 節 |
| 可否證預測 | 第 7 節 |
| EGTS/CCPO v0.4 候選整合、未來實作映射問題 | 第 8 節 |
| 假設、限制及尚未建立的 Temporal applicability | 第 9 節 |

## 證據分類與限制

- Model-inferred：在明列抽象模型內定義或推導，不是 Temporal runtime 觀察。
- Source-confirmed：原報告的一手外部文件支持；引用 URL 原樣保留。此次保存不冒充重新執行文件驗證或實驗。
- Unproved assumptions：環境公平性、durability、效果端原子去重與 authority 檢查等依命題明列條件。
- Unknown：實際 Temporal 的 commit/response 路徑、不可區分世界、外部 exactly-once 或具體產品 bug；C-1 沒有建立這些結論。

本目錄沒有程式碼、fixture 或新 runtime logs。舊 C2-D/C2-E2 的 PASS 不能轉作本報告的 runtime 證據。原始來源是對話而非可重新執行的實驗；可重現的是文件完整性與推論審核。

## 外部參考資料

僅保存原報告引用，不複製文章或論文全文：

- [AWS Builders’ Library — Making retries safe with idempotent APIs](https://aws.amazon.com/builders-library/making-retries-safe-with-idempotent-APIs/)
- [AWS — Transactional outbox pattern](https://docs.aws.amazon.com/prescriptive-guidance/latest/cloud-design-patterns/transactional-outbox.html)
- [Microsoft — Idempotent consumer pattern](https://learn.microsoft.com/en-us/azure/architecture/patterns/idempotent-consumer)
- [Chubby paper, §2.4](https://storage.googleapis.com/gweb-research2023-media/pubtools/4444.pdf)
- [Stripe — Idempotent requests](https://docs.stripe.com/api/idempotent_requests)

## 完整性與公開範圍

從本目錄執行 `sha256sum -c SHA256SUMS` 即可檢查所有新研究文件；manifest 本身由 Git commit 保護，不包含自身雜湊。`provenance.json` 記錄父提交、來源與保存範圍；`preservation-audit.json` 記錄舊證據雜湊核對結果及公開內容檢查。

只加入本目錄文件；不包含 build cache、target、環境變數、憑證、私人金鑰或機器專屬設定。報告沒有需要刪改的私人路徑，因此未製作經過刪改的公開替代稿。
