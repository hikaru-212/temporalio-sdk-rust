此為原始執行前登記文件的公開副本；下文當時狀態完整保留，不代表本次重新執行。

# Negative-only preregistration

此檔在測試啟動前保存。只執行既有 TestCCPOC5B2Negative，source與P2/Main/Positive相同；無source修改，無Main/Positive rerun，無其他情境。

預期：controlled store persistence.TimeoutError 在舊delegate實際invocation前回傳；delegate goroutine可已建立但等待launch channel。真正uncertain-write handling啟動renewal，持久RangeID1→2。guarded fresh read先確認revision3、NextEventId4、pending K=(Scheduled2,Started3,Attempt1)。只有此後才啟動原RangeID1的real persistence request。

舊delegate可先成功history append，但真正SQL shard read會看到2，pinned range validation回傳persistence.ShardOwnershipLostError，應在pre-mutation gate與UpdateCurrentExecutions之前拒絕。預期gateVisits0、mutations0、commits0、rollbacks1、context未提前取消且transaction closed/done已確認。

stale attempt前後，持久RangeID2、revision3、原pending K、effective history與ActivityInfo以及六類全範圍task ledger應相同；physical history append不代表valid acceptance。正常server path以原有效WFT completion重試，應只有一次有效Completed4／ActivityScheduled5、revision3→4、一筆Activity transfer task與正常Activity polling；不執行external Activity。

ordering、build、fixture、cleanup或evidence缺漏分類INCONCLUSIVE；產品RED需要獨立確認契約違反，不可由注入error或預期stale rejection推斷。若阻塞，保存輸出並停止，不自動改source或新增情境。

這是controlled manager/store ambiguous result模擬；不是native SQLite timeout、一般correctness theorem或external exactly-once驗證。
