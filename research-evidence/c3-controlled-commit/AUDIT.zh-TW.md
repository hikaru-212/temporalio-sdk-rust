# C-3 checkpoint：獨立證據範圍審核

本文件是保存 checkpoint 時新增的來源／證據審核，沒有新增 runtime 結果，也沒有回寫舊報告。原 REPORT.zh-TW.md 與其 CONFIRMED SAFETY 分類按歷史原文保存，閱讀時需同時採用以下精確範圍。

## persisted-task 查驗的依賴

**Source-confirmed：** 最終測試 `server-test/commit_ambiguity_test.go:157` 的 `inspect()`：

1. 由真實 ExecutionManager 讀取 Mutable State 與 DBRecordVersion，另讀 shard RangeID。
2. 以直接讀得的 current branch token 和 `NextEventId`，讀取有效 history；檢查 history pagination 已結束、completion／ActivityScheduled 數量與 causal references。
3. 呼叫 `HistoryTaskRecorder.MatchTasksForWorkflow(CategoryTransfer, ns, workflow, run, ActivityTask filter)` 發現 IDs。
4. **只對上述每個已發現 ID** 呼叫 `GetHistoryTasks`，範圍為 `[id, id+1)`，再斷言 recorded / returned tasks 各一筆。
5. 後來增加的 `all_successful_task_write_records` 也取自 `GetAllRecordedTasks()`，不是另一份 persistence enumeration。

**Source-confirmed：** pinned [HistoryTaskRecorder.UpdateWorkflowExecution](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/tests/testcore/history_task_recorder.go#L83) 先執行 delegate，僅在 `err == nil` 時記錄傳入 mutation 的 tasks。這是真實寫入成功後的 instrumentation，不是獨立搜尋 DB 中全部 tasks 的機制。

**Runtime-supported：** 八次歷史紀錄證實各 scenario 的指定有效 history、MS 狀態、已發現 Activity task row 與正常 polling recovery。原先的 DB row 查驗確實讀取 persistence，並非只相信 mock success。A 的 revision4／history 在 retry 前後不變；B 的拒絕後仍為 revision3，K 有效，解除 fault 後成功；兩次內部 rejected-store calls 也保留。

**Model-inferred：** 「已發現 task 確實存在於 DB」與「DB 的完整相關範圍只有這個 task」是不同命題。現有查驗支持前者；後者仍依賴 recorder 發現完整性等假設。尤其 B 的 recorded list 為空時，task 查詢迴圈根本不執行，不能把這解讀成獨立全範圍 DB absence 證據。直接讀得的 MS／有效 history 仍可支持當時沒有有效 WFT completion／ActivityScheduled，兩種證據不要混為一談。

**Unknown／剩餘義務：** 若要強化「不存在未被 recorder 發現的額外 durable tasks」，需另行建立與 recorder IDs 無關的 shard/category/watermark 範圍，逐頁列舉、完整處理 continuation tokens，並控制 task consumption／deletion，使觀察窗口一致。還需依 workflow/run 與 causal identity 篩選，涵蓋相關 task 類別。現有細範圍 task query 沒有對整個相關範圍列舉，也沒有對該整體範圍證明 EOF。本次只記錄此義務，未新增測試、fixture、DB query experiment 或結果。

## Fault hooks 與測試來源

全部 source references 固定 Server `0d5e3848bed625907987f73d5eaae8dc77bc6590`；最終 Go source SHA-256 `22b5bf7cd197edfaa78452452e1f239e8d7d4997aceefc9301b95b79ca213f25`。完整 fault 函式定義保留在該 test source 與 test-only.diff。

| 用途 | 真實 hook／契約 | 測試行為與界限 |
|---|---|---|
| A 回應抑制 | [post-handler gRPC interceptor](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/common/rpc/grpcfaults/interceptor.go#L24) | 只在目標 completion handler 成功後回傳 injected Unavailable；不是實體斷線或封包遺失。 |
| B precommit rejection | [ExecutionStore wrapper](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/common/persistence/faultinjection/execution_store_gen.go#L251)、[fault inject](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/common/persistence/faultinjection/fault.go#L61) | 在 delegate store 前拒絕；沒有進入 SQL history append 或 MS transaction，不是 commit 中途的未知結果。 |
| task 查驗窗口 | [HistoryTransferTaskInterceptor](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/service/history/transfer_queue_active_task_executor.go#L108) | 以 channel 暫停目標 Activity transfer task；解除後正常 poll。不是 crash/restart。 |
| 正常 WFT validation | [completion handler](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/service/history/api/respondworkflowtaskcompleted/api.go#L203) | 使用真實已 started task token；重送 protobuf bytes 相同。 |

四個 logical scenarios 為 Control、提交成功但 response 被抑制、precommit failure 後重試、同 K 並行完成。run-01～04 是原執行；run-05～08 只補全 recorder ledger，並未完成上述獨立全範圍 enumeration。八次原 exit code 全為 0；lint 工具準備 exit 2，保留原錯誤，絕不標成 lint PASS。

## 保留的限制

- SQLite only，file WAL／synchronous=normal；沒有其他 backend 或 power-loss durability 結論。
- Normal、已 started 的 WFT only，單一 ScheduleActivityTask command。
- Controlled post-handler response suppression；controlled precommit／pre-store failure。
- 沒有 mid-transaction uncertain persistence result，也沒有 history-only append 的 failure arm。
- 沒有 crash recovery、restart、ownership transfer 或 multi-cluster failover。
- 沒有 external Activity exactly-once；polling 只證明存活 cluster 的工作可派送。
- A/B error types 與 fault messages 不同；沒有證明完整 Worker observations 相等。
- 有限排程 witness，不是一般 Temporal correctness theorem；並行測試未獨立證明兩個 SQL transactions 同時到達 CAS。
- 完整 relevant task range 的獨立 enumeration 尚缺，不能靠單筆 DB row 命中補足。

以上是證據邊界與後續義務，不是新發現的 Temporal correctness bug，也不是修改原實驗歷史結果。
