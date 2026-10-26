此為原始執行前登記文件的公開副本；下文當時狀態完整保留，不代表本次重新執行。

# Main-only preregistration

此檔在 go test 啟動前保存。此輪只有 TestCCPOC5B2Main，固定 P2 source hashes，不更改 test source，不執行 Positive／Negative／Phase 0／C-5B.1。

預期：old transaction 真實讀到 RangeID 1，停在 mutation 前；store 回傳受控 TimeoutError，真實 uncertain-write recovery 完成 RangeID 2 renewal；guarded fresh read 回傳 revision 3、NextEventId 4、pending WFT K=(Scheduled2, Started3, Attempt1)。舊 transaction 在 fresh read 後仍可重讀 range1，尚無 mutation；接著釋放 gate，真正 SQL mutation 應失敗（候選 extended code 517），rollback、delegate done 與未提前取消的 context 應獲確認。

release 前後独立 persisted MS、effective history、ActivityInfo、六類 task range ledger 應相同；physical history append 不算有效接受。identical valid retry 應造成 revision3→4、唯一 Completed4／ActivityScheduled5、ActivityInfo 與 durable Activity task；正常 Activity polling 成功，但不執行 external Activity code。

PASS 需時序、persisted evidence 與 cleanup 均成立；exit0 本身不充分。build、fixture、hook、context、join 或 evidence 缺漏分類 INCONCLUSIVE。RED 需獨立確認產品契約違反。若失敗，只保存診斷，不自行改 source 或新增情境。

這仍是 manager/store early-return 模擬，非 native DB commit timeout；只驗證 Main，controls 未執行，所以不宣稱完整 C-5B.2 CONFIRMED SAFETY。
