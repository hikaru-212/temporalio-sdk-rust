此為原始執行前登記文件的公開副本；下文當時狀態完整保留，不代表本次重新執行。

# Positive-only preregistration

在 test 啟動前保存。固定 pinned SHA 與 P2 source hashes，不更改 test source；只執行 TestCCPOC5B2Positive，無 Main、Negative 或其他測試。

預期順序：同一舊 transaction 真實讀取 RangeID1 → mutation → Commit成功且 delegate done → 回傳受控 persistence.TimeoutError → 真實 uncertain-write recovery → RangeID2 durable renewal → guarded fresh read revision4/NextEventId6/無 pending WFT。此時 effective history 應含唯一 Completed4 與 ActivityScheduled5；commit count1、rollback0、mutation1。

Identical WFT retry 應被拒絕（此既有 harness 斷言 NotFound），有效 history、Mutable State revision與六類完整 task ledger 不變。此 NotFound 不是原 commit outcome 的獨立證明；需 Commit實際結果與 persistence reads。Activity polling 應成功，無 external Activity 執行。cleanup 必須 join delegate，再停止服務與關閉物理 connections。

ordering／fixture／build／cleanup 或 persistence evidence 缺漏 → INCONCLUSIVE，不當作 product RED。只有獨立確證產品契約違反才可分類 RED。沒有任何自動修 code、重跑 Main 或擴展情境。

此為受控 manager/store 早回傳 error 的 Positive control，非 native SQLite commit-timeout；Negative 未執行，不能宣稱完整 C-5B.2 safety 或一般 correctness theorem。
