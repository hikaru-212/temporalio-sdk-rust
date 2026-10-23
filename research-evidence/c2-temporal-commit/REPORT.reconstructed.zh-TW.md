**C-1 已成功保存並推送；C-2 的結論是：`RespondWorkflowTaskCompleted` 存在提交結果不確定的窗口，但正常 WFT 路徑具備防止重複接受命令與恢復進度的機制。本輪支持「可被協定容忍的 ambiguity」，沒有建立 Temporal correctness bug。**

以下區分 **Source-confirmed**、**Model-inferred**、**Runtime-supported** 與 **Unknown**。

**1. C-1 checkpoint 已完成。**

| 項目 | 結果 |
|---|---|
| 分支 | `research/ccpo-egts-c1-ambiguous-commit` |
| Commit | `029a790e4fa7663cf70c8fedc82b379d47f99dbb` |
| Parent | `027c13363515d9e4807371777ca510fe00ee7f68` |
| 推送目的地 | `https://github.com/hikaru-212/temporalio-sdk-rust.git` |
| 遠端驗證 | GitHub owner 與登入帳號均為 `hikaru-212`；遠端 SHA 與本地一致 |
| 工作目錄 | 新建獨立 clone，提交後乾淨 |

[查看已保存的 checkpoint](https://github.com/hikaru-212/temporalio-sdk-rust/commit/029a790e4fa7663cf70c8fedc82b379d47f99dbb)。

只提交 `research-evidence/c1-ambiguous-commit/` 下的五個文件：

- `REPORT.reconstructed.zh-TW.md`
- `README.md`
- `provenance.json`
- `preservation-audit.json`
- `SHA256SUMS`

報告明確標為**依對話重建的完整稿**，沒有冒稱既存磁碟文件或平台原始匯出。原有九節、推論、限制及五個外部引用均保留。

完整性檢查：C2-D **47 項**、C2-E2 **96 項**既有證據，以及四份新文件的雜湊全部通過。C-1 報告 SHA-256：

```text
2bff3cf0c28096c31dedcfd1540aa6c2425f678f5bf0694b47118e620c703507
```

遠端 C2-D／C2-E2 仍分別為 `899ccfb…`／`027c133…`。既有 review worktree 的未追蹤研究文件保持原狀。沒有 force push、PR、issue 或留言。

**2. C-2 固定的版本與適用範圍。**

〔Source-confirmed〕本輪讀取：

- Temporal Server：`0d5e3848bed625907987f73d5eaae8dc77bc6590`，本地 checkout 乾淨。
- SDK-Rust：`8f4a676f9e1a6276b5cc2f77e25c9183ce256fe9`；核對所引用的 client／completion production files 與該 SHA 無差異。

這是明列版本的來源映射，**不是已部署組合的 runtime 驗證**。

主模型限定為：有效、已 Started 的正常 WFT，合法命令，單一 active namespace 執行路徑，以及單頁或最後一頁 completion。以一般 `ScheduleActivityTask`、非 eager dispatch 作具體例子；不把外部 Activity 執行納入同一交易。

兩個重要例外：

- 中間 completion page 可以只存入 workflow context 快取就回傳成功。
- 部分 speculative WFT 可以略過持久化，並藉 `ResetHistoryEventId` 通知 SDK 回退。

因此，**不能無條件定義 `RPC OK ⇒ durable WFT completion`**。來源：[pagination 與 speculative 分支](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/service/history/api/respondworkflowtaskcompleted/api.go#L676)。

**3. 真實呼叫鏈與提交位置。**

下表均為〔Source-confirmed〕：

| 層次 | 檔案／函式與責任 |
|---|---|
| Worker → Frontend | [`WorkflowHandler.RespondWorkflowTaskCompleted`](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/service/frontend/workflow_handler.go#L1220)：驗證請求、取得 namespace ID、呼叫 History |
| History routing | [`clientImpl.RespondWorkflowTaskCompleted`](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/client/history/client_gen.go#L1188)：解析 token，以 namespace／workflow 計算 shard，透過 redirector 呼叫 owner |
| History RPC entry | [`Handler.RespondWorkflowTaskCompleted`](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/service/history/handler.go#L558)：解析 token、取得 shard engine |
| Engine | [`historyEngineImpl.RespondWorkflowTaskCompleted`](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/service/history/history_engine.go#L616)：建立 handler，呼叫 `Invoke` |
| 驗證及命令處理 | [`WorkflowTaskCompletedHandler.Invoke`](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/service/history/api/respondworkflowtaskcompleted/api.go#L112)：取得 workflow lease、驗證目前 WFT、建立 completed event、處理 commands |
| 關閉記憶體交易 | [`ContextImpl.UpdateWorkflowExecutionAsActive`](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/service/history/workflow/context.go#L695) → `CloseTransactionAsMutation`：產生 events、Mutable State mutation、tasks 及版本條件 |
| 提交要求 | [`TransactionImpl.UpdateWorkflowExecution`](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/service/history/workflow/transaction_impl.go#L167) → [`shard.ContextImpl.UpdateWorkflowExecution`](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/service/history/shard/context_impl.go#L597)：設定 `RangeID`、task keys，送 persistence |
| Persistence | [`executionManagerImpl.UpdateWorkflowExecution`](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/common/persistence/execution_manager.go#L168)：序列化並呼叫 backend |
| 回覆 | [`Invoke` 後段](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/service/history/api/respondworkflowtaskcompleted/api.go#L795)：提交後組裝下一個 WFT／Activity response；Frontend 再回傳 worker-facing response |

關鍵時間線是：

```text
語言完成 activation
→ Core 整理 commands 並送 RPC
→ Server 驗證 token／目前 WFT
→ 修改記憶體 Mutable State、建立 events／tasks
→ append history
→ 條件式提交 Mutable State ＋ durable tasks
→ 建構 response
→ 傳送 response
→ Worker 收到成功 acknowledgement
```

**`CloseTransactionAsMutation` 不是資料庫 commit；handler 回傳也不是 Worker 已收到 acknowledgement。**

Backend 差異如下：

| Backend | 可定位的提交邊界 | 限制 |
|---|---|---|
| SQL | 先 append history，再於 shard row lock、execution version 條件下更新 Mutable State／tasks，最後 `tx.Commit()` | History append 與 Mutable State 不是同一交易；實際 durability 仍依 DB／部署設定 |
| Cassandra | 先 append history，再以 conditional logged batch／`MapExecuteBatchCAS` 提交 Mutable State、tasks 與 `RangeID` 條件 | 依 Cassandra LWT／consistency 契約；預設為 `LOCAL_QUORUM`、`LOCAL_SERIAL`，不能延伸成任意跨區同步保證 |

來源：[SQL 寫入順序](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/common/persistence/sql/execution.go#L334)、[SQL commit](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/common/persistence/sql/common.go#L52)、[Cassandra CAS](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/common/persistence/cassandra/mutable_state_store.go#L689)、[consistency defaults](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/common/config/persistence.go#L216)。

〔Model-inferred〕在上述 persistence 契約成立時，正常 WFT 的**有效狀態轉移線性化點**可對應 SQL 交易成功提交或 Cassandra conditional batch 生效，而非 history blob 單獨寫入。

Server 架構文件亦明確以 Mutable State 所反映的 history 範圍判定有效 events；未成功提交 Mutable State 的尾端 history，不能單獨充當 completion receipt。[一致性契約](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/docs/architecture/history-service.md#L318)

**4. 三種完成，以及 operation／attempt／ownership 身分。**

〔Source-confirmed〕必須分開：

1. 語言／Core 完成本地 activation。
2. Server 持久接受該 WFT 的命令。
3. Worker 收到成功 RPC response。

Core 的 completion error handler 可以要求 eviction，之後 `activation_completed` 仍回傳 `Ok(())`；內部 `WFTReportStatus::Reported` 也不能直接解讀為 durable success receipt。[Core completion 流程](https://github.com/temporalio/sdk-rust/blob/8f4a676f9e1a6276b5cc2f77e25c9183ce256fe9/crates/sdk-core/src/worker/workflow/mod.rs#L616)

| C-1 概念 | 本路徑的對應 |
|---|---|
| 邏輯 completion identity | namespace、workflow、run，以及該 WFT 的 scheduled／started identity |
| WFT attempt | token 的 `Attempt`；Server 用來比對目前 WFT |
| RPC attempt | 同一 completion 的傳輸重送；**不等於**新的 WFT attempt |
| Payload | commands、protocol messages 與 completion metadata |
| Server ownership | shard `RangeID`，另有 active namespace／version 檢查 |
| Worker authority | 該 token 是否仍對應目前可完成的 WFT |
| Durable evidence | 有效 history、Mutable State revision、pending work／durable tasks |

Token 包含 scheduled ID、started ID、attempt、version、started time 與 vector clock；Server 比對目前 WFT，而非只檢查 token 能否反序列化。[token 定義](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/proto/internal/temporal/server/api/token/v1/message.proto#L39)、[驗證條件](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/service/history/api/respondworkflowtaskcompleted/api.go#L203)

**Shard 換 owner 不必然使所有舊 RPC token 失效。** 晚到 RPC 可以由新 owner 重新驗證；若 WFT 仍有效，它仍可能被接受。被 fencing 的是使用舊 `RangeID` 的持久化寫入，兩者不能混為一談。

**5. 重試、防重複與恢復保障。**

〔Source-confirmed〕SDK client 的 retry factory 複製原 request，保留相同 token 與命令內容。此 pinned 版本：

- `Unavailable` 等列出的 transient codes 可在 retry budget 內重試。
- 普通 completion RPC 的 `DeadlineExceeded` 不在一般 retry code 清單。
- `NotFound` 不會被當成可重試成功；Core 要求 `TaskNotFound` eviction。
- 其他未處理的 completion error 也會要求 eviction。

來源：[request cloning](https://github.com/temporalio/sdk-rust/blob/8f4a676f9e1a6276b5cc2f77e25c9183ce256fe9/crates/client/src/grpc.rs#L135)、[retry policy](https://github.com/temporalio/sdk-rust/blob/8f4a676f9e1a6276b5cc2f77e25c9183ce256fe9/crates/client/src/retry.rs#L16)、[eviction](https://github.com/temporalio/sdk-rust/blob/8f4a676f9e1a6276b5cc2f77e25c9183ce256fe9/crates/sdk-core/src/worker/workflow/mod.rs#L790)。

Server 的保障包含：

| 保障 | 實際作用 |
|---|---|
| 目前 WFT 檢查 | 已完成、已 timeout 或 attempt 不匹配的 completion 被拒 |
| 完成時清除 pending WFT | 同 token 重送不再找到原可完成 WFT |
| Workflow lease／版本條件 | 避免並行或過期 Mutable State 重複提交 |
| `RangeID` fencing | 阻擋失去 shard ownership 的持久化寫入 |
| 出錯清除 cache | 後續處理重新取得 durable state |
| WFT timeout／新 task | 未完成 WFT 可由 timer 驅動後續恢復 |
| Durable tasks | 已提交的後續工作不依賴原 completion response 被收到 |

來源：[清除 WFT](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/service/history/workflow/workflow_task_state_machine.go#L1107)、[SQL version check](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/common/persistence/sql/execution_util.go#L629)、[cache 清除](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/service/history/workflow/context.go#L888)、[timeout recovery](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/service/history/timer_queue_active_task_executor.go#L382)。

這是**依狀態拒絕重複接受**，不是「保存每次 RPC response，重送時回傳原 response」的 receipt cache。`NotFound` 本身不能區分「之前完成」與「之前 timeout」。

更直接對應 C-1 的是 persistence uncertainty：`handleWriteErrorLocked` 明確區分確定未提交的錯誤，以及可能已提交的錯誤；後者觸發 shard reacquisition。成功更新 `RangeID` 後，舊寫入必須已生效並可讀，或無法再以舊 range 生效。[不確定寫入處理](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/service/history/shard/context_impl.go#L1501)、[range renewal](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/service/history/shard/context_impl.go#L1164)

〔Model-inferred〕這提供了 C-1「查到 Absent，但舊寫入可能稍後提交」問題所欠缺的**Server 內部持久化屏障**。它不代表任意公開查詢的 Absent 都會取消仍在途的 RPC。

**6. 兩個候選不可區分世界。**

以下是〔Model-inferred〕、有來源支持的協定允許路徑，**不是已執行的故障注入結果**。

共同前提：同一有效正常 WFT、同一 commands／token、本地 activation 已完成；比較時間點之前，沒有其他 task delivery 或 history query 結果抵達 Worker。

| 世界 A | 世界 B |
|---|---|
| 請求通過驗證 | 請求在進入持久化前延遲或未送達 |
| History append 成功 | 沒有有效 completion commit |
| Mutable State／tasks commit 成功 | 原 WFT 尚未被此請求完成 |
| 成功 response 遺失或延遲超過 deadline | Worker 同樣沒有收到 response |
| Worker 觀察 timeout | Worker 觀察相同 timeout |

可達性的依據：

- Server 明確先提交，再建構及回傳 response；兩者之間存在獨立步驟。
- `newDetachedContext` 會拒絕已取消的輸入 context，但建立後不隨 caller cancellation 立即取消；timeout 不是 rollback 證據。[context 行為](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/service/history/shard/context_impl.go#L2414)
- gRPC 契約明確允許狀態變更已成功，caller 仍收到 `DEADLINE_EXCEEDED`。[gRPC status contract](https://grpc.io/docs/guides/status-codes/)

令 Worker 的完整可用觀察為：

\[
Obs_W=\langle
本地狀態、activation、commands、token、RPC嘗試、
收到的status/metadata/payload、timer、eviction、
已完成的poll/query結果
\rangle .
\]

安排兩世界在 deadline 前收到相同訊息序列、相同本地排程與時間讀值，且沒有額外證據抵達，則：

\[
Obs_W(A_{\le T})=Obs_W(B_{\le T}),
\qquad
Committed(A_{\le T})\ne Committed(B_{\le T}).
\]

這是**有限觀察前綴的不可區分性**。沒有刪除 history API，也沒有聲稱它們在所有未來都不可區分。後續查詢或 replay 可以增加證據。

此外，具體 `ScheduleActivityTask` 例子中，A 可以是「durable task 已建立、外部 Activity 尚未執行」。不能把 A 的 Server commit 自動等同 C-1 的外部效果已發生。

**7. 同一恢復行動是否能在兩世界安全？**

〔Model-inferred〕在正常 WFT、相同 request、正確 persistence 與 ownership 契約下，重送同一 completion：

```text
A：原 WFT 已完成 → token/current-task 檢查拒絕 → 不再接受同一命令批次
B：原 WFT 仍有效 → 可以提交一次
   原 WFT 已失效 → 拒絕，交由後續 task/replay 恢復
```

因此，安全行動不需要 Worker 先知道第一次 request 是否成功。

可提出受限安全命題：

\[
\#ValidDurableAcceptances(K)\le 1
\]

其中 \(K\) 是同一正常 WFT identity。證明義務是：完成轉移移除 pending WFT；重載維持該事實；並行舊快照受 revision 條件約束；舊 owner 寫入受 range fencing 約束。這不是對所有外部效果的 exactly-once 命題。

公開 `GetWorkflowExecutionHistory` 能提供相符 scheduled／started references 的 completed event 及命令事件，並依 Mutable State 的 branch／event range 讀取。[history 讀取範圍](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/service/history/api/getworkflowexecutionhistory/api.go#L273)

但應區分：

- **正面證據：** 找到同一 run／有效 branch 的匹配 completion。
- **暫時缺席：** 尚未找到，不能單獨證明舊 RPC 永遠不會提交。
- **自動恢復：** Core 的 completion error 路徑主要採 eviction／後續 task；本輪沒有找到它必定主動查清原 RPC outcome 的保證。
- **條件式進度：** 依賴 persistence 恢復、有效 ownership、timer／queue 公平處理、Worker 可用及 replay 正確性。

**8. 外部副作用與剩餘義務。**

〔Source-confirmed〕一般 `ScheduleActivityTask` 先建立 scheduled event、ActivityInfo 與 durable transfer task；真正外部工作在後續 dispatch／執行發生。[命令處理](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/service/history/api/respondworkflowtaskcompleted/workflow_task_completed_handler.go#L555)、[task generation](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/service/history/workflow/task_generator.go#L552)

`RecordMarker` 保存的是 marker event；它不把已發生的 Local Activity 效果與 Server commit 合併成跨系統原子交易。程式中的 `effects.Apply()` 亦是延後執行的 callbacks，不能等同外部效果 ledger；其架構文件明言不提供交易保證。[effect package](https://github.com/temporalio/temporal/blob/0d5e3848bed625907987f73d5eaae8dc77bc6590/docs/architecture/effect-package.md#L13)

因此仍為〔Unknown／未證明〕：

- 任意 Activity／Local Activity 外部副作用不重複。
- 所有 SQL／Cassandra 部署設定下的 durability 與 failover 保證。
- 跨 active-cluster 切換、reset、retention／刪除後的完整結果辨識。
- eager response 遺失、所有 speculative／pagination 分支的完整 liveness。
- 永久故障下的無條件完成。
- 此版本組合在實際故障注入下的觀察結果。

本輪沒有新增〔Runtime-supported〕證據；既有 C2-D／C2-E2 PASS 不涵蓋這些 commit 故障窗口。

**9. 對 CCPO 的結果。**

〔Model-inferred〕C-1 適用於這條路徑的**知識模型**：本地完成、durable commit、ack received 確實不同；Worker 在某個時間點可以無法分辨已提交與未提交。

但 C-1 的受限不可能命題不能直接套用：Temporal 並非「每收到一次 retry 就獨立執行一次效果」的基準處理器。它有 task identity、目前狀態檢查、條件式提交、ownership fencing、durable work 與 replay recovery。

CCPO 應分別記錄：

- `AttemptRPC`、`CommitWorkflowTransition`、`GenerateDurableTask`、`ExecuteExternalEffect`、`ReceiveAck`。
- Worker belief 與 Server 的實際 durable state。
- 能辨識歷史結果的證據，以及能讓未知結果下的相同行動安全的保障。
- RPC／page／WFT／run／shard 等證據作用範圍。
- Safety 的條件與 recovery／progress 的額外假設。

**已建立的是來源支持的 commit uncertainty 與受保護恢復機制；尚未建立的是具體 Temporal correctness failure、外部 exactly-once 或一般性端到端正確性。** 已停止於 C-2；Stage 2 全程唯讀，沒有編譯、測試、新 fixture 或 GitHub 寫入。