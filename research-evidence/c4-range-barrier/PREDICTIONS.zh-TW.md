# C-4 預登錄（實作及執行前）

分類：Model-inferred predictions；尚無本輪 runtime 結果。
Server: 0d5e3848bed625907987f73d5eaae8dc77bc6590。
來源：C-3 testcore 真實 onebox/file SQLite helpers；本輪使用獨立 clone、獨立測試。

## 範圍與 hook
正常已 started WFT K=(namespace,workflow,run,scheduled=2,started=3,attempt=1)。單一 ScheduleActivityTask、無 eager、speculative、pagination、Activity 外部執行。
既有 temporal.WithPersistenceFactoryProvider + TestClusterConfig.AdditionalServerOptions，僅 History manager 包裝；未改 production。
ExecutionManager.UpdateWorkflowExecution 最外層（真實 persistence factory 完整 delegate 外面、shard ContextImpl 下面）一次性注入相同 *persistence.TimeoutError。
A 呼叫 delegate 並要求真正成功後注入；B 不呼叫 delegate 即注入。因此 B 是 before-delegate no-commit 模擬，非失敗 SQL COMMIT。
兩者是受控結果模擬，不是物理 database commit-timeout 重現。
ShardManager.UpdateShard 包裝以 channel 暫停 recovery range renewal，記錄 stack、old/new range；release 後呼叫真正 delegate 並直接讀 durable shard row。
不從 log 或 delegate success 單獨推論提交；獨立 ExecutionManager 讀 MS、有效 history、task ranges。
為避免 enumeration 期間 queue ack 刪除 row，僅延後真正 CompleteHistoryTask/RangeCompleteHistoryTasks delegate；不偽造成功。Activity transfer dispatch 另以既有 namespace testhook 延後。完成觀測後解除，PollActivity 驗證可交付但不執行 Activity。

## 預測
Control：Range 不變，一次接受；MS revision 3→4、NextEventID 4→6，有效 WFTCompleted4、ActivityScheduled5 各一次、pending WFT 清除、Activity transfer 可恢復。
Arm A：delegate 成功；同一 caller 收 Timeout；actual uncertain-write path 進 reacquisition；new durable RangeID > old；barrier 後 MS 含一次有效完成/Activity 與 durable tasks；原 token 重試不得第二次接受（預期 NotFound，但 status 非提交證明）；無第二次 revision；可 PollActivity。
Arm B：delegate 未被呼叫；同樣 Timeout/reacquisition；barrier 後 MS revision/有效 history 維持 pending K；以 byte-identical token/commands 重試，若 K 仍有效成功一次；若實際合法失效則必須另記錄正常 recovery，不能硬當 PASS。不得重複接受；可 PollActivity。
A/B 第一次 frontend RPC 預期 DeadlineExceeded（source converts persistence.TimeoutError）；若 upstream 自動重試，必須記錄而不得把成功 ack 偽裝成 lost ack。
每次 enumerate transfer/timer/visibility/replication/archival/outbound 的完整 [MinimumKey,MaximumKey) 範圍直到 page token 空，保留範圍、頁數及目標 workflow rows。task ID discovery 不依賴 recorder。列舉非跨類別 SQL snapshot，需維持目標 workflow 無其他 mutation 的穩定觀察窗。

## 分類與可否證性
有效 A/B 若 >1 durable acceptance 或 committed work 遺失／未提交 K 被假完成 → CORRECTNESS RED。
編譯、hook 未命中、delegate error、無法證明 range barrier、invalid token、無獨立 persistence 證據 → INCONCLUSIVE。
正確單次接受、有效 recovery → CONFIRMED SAFETY（僅上述模型/SQLite）。
optional C 不實作：目前選定 manager seam 可暫停 delegate 前或後，但沒有已驗證的 SQL transaction 內 hook 可證明真正在途舊 range write 與 acquisition 競賽。source fencing obligation 保留 UNKNOWN/runtime 未驗證；不以 delayed-before-delegate 假裝 SQL in-flight。
failed reacquisition shutdown 僅 source analysis，不擴展 fault arms。

## 必須保留的限制
非 crash/power-loss durability；非 owner transfer；非 physical commit timeout；非 Cassandra/Postgres；非 external exactly-once；不證明 A/B Worker 完整觀察相同；非一般 Temporal 正確性定理。
