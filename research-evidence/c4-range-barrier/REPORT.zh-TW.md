# CCPO C-4：Persistence Commit Uncertainty 與 RangeID Barrier

本輪 Control、Arm A、Arm B 均為 **CONFIRMED SAFETY（限定本次受控 SQLite 模型）**。A/B 均走實際 uncertain-write recovery，durable RangeID 由 1 變成 2；A 保留原提交並拒絕重複完成，B 保留 pending WFT 並容許原請求成功一次。沒有 correctness RED。

**未證明**底層未知交易仍在執行時的完整 fencing 性質：本輪 A 的 delegate 已結束、B 的 delegate 未執行。Optional C 未做，相關競賽為 UNKNOWN。這不是物理 database commit-timeout 重現，也不是一般 Temporal 正確性定理。

證據標記：**Source-confirmed**＝讀取固定版本程式或具名外部規格；**Runtime-supported**＝本輪真實 testcore / SQLite 執行與獨立 persistence 觀測；**Model-inferred**＝明示假設下的推論；**Unknown**＝未驗證。

## 1. 基線與完整性

- [Source-confirmed] Server `0d5e3848bed625907987f73d5eaae8dc77bc6590`，獨立 detached clone：`<SERVER_CHECKOUT>`。
- [Source-confirmed] C-3 checkpoint `632c157c02af7154e4b93541c51dc1d17141c003` 保留；原 C-3 manifest 58 個項目重新核對通過。未重跑 C-3。
- [Source-confirmed] 唯一新增 Server 測試：`tests/ccpo_c4/range_barrier_test.go`；最終 SHA-256 `a6fd789b274808c0a807555795ee3598de935694c64bec6af26202a795f02223`。沒有 tracked production diff。
- [Runtime-supported] file SQLite，`cache=shared, journal_mode=wal, synchronous=normal, busy_timeout=30000`；單一 history shard、停用系統 worker；真實 frontend/history/matching/persistence，沒有外部 Activity executor。
- [Source-confirmed] Go 版本、平台、driver dependency 與 25 個已核對 pinned source 雜湊見 `source-provenance.json`。首次 build 前可用磁碟約 20 GiB。
- [Source-confirmed] 本文、manifest、逐 run 原始 stdout/stderr、exit code、測試當時版本、protobuf、精確 diff 均在本目錄。沒有 commit、push 或 GitHub 寫入。

## 2. 真實 uncertain-write lifecycle

下列路徑均相對 pinned Server 根目錄，行號依該 SHA。

| 位置 | Source-confirmed 契約／行為 |
|---|---|
| `service/history/shard/context_impl.go:597` `UpdateWorkflowExecution` | 取得 IO semaphore，鎖內檢查狀態、追蹤 task keys，寫入目前 RangeID；解鎖呼叫 execution manager；先呼叫 request completion tracker，再將結果交給 `handleWriteError`。 |
| 同檔 `:1501` `handleWriteErrorLocked` | 若 request range 已非目前 range，或 context 不再 valid，直接回原錯誤；不重新宣稱該寫入成敗。 |
| 同檔 `:1513` 起錯誤分類 | nil 成功；AppendHistoryTimeout 單獨處理為可重試 history append。ConditionFailed/WorkflowConditionFailed/CurrentWorkflowConditionFailed、InvalidPersistenceRequest、TransactionSizeLimit、ResourceExhausted、NotFound、NamespaceNotFound 屬確定未提交，不啟動 uncertain recovery。ShardOwnershipLost 觸發停止。其餘型別走 Lost→Acquiring。 |
| 同檔 `:1130`, `:808`, `:267` | Acquiring/Initialized 的 `errorByState` 回 `Unavailable("shard status unknown")`；MS persistence reload 與寫入路徑會檢查此狀態。`GetEngine` 仍使用已存在的 engineFuture，不能概括為整個服務停止接收所有 RPC、所有 cache read 或所有 in-flight operation。 |
| 同檔 `:1671` transition、`:2030` acquireShard | shard context 擁有 recovery goroutine；由 Acquired 轉 Acquiring，load metadata，再 renew range；成功後回 Acquired，通知 queue processors。controller 負責移除／停止失效 context。 |
| 同檔 `:1164` renewRangeLocked | 在 shard 鎖下 drain tracked task requests，複製 shard info、RangeID+1，以 PreviousRangeID 條件呼叫真實 UpdateShard；成功才更新本地 range 與 task-key manager。 |
| `service/history/shard/task_request_tracker.go:40`、`:110` | request 回傳後 inflight count 遞減；未知結果的 task key 仍保留 pending。**drain 完成表示 manager 呼叫已返回，不等於遠端未知交易必定停止。** |
| `service/history/shard/task_key_manager.go:72`、`:88` | 新 range 成功後重設 key generator、清除 pending tracker，reader high watermark 可前移；其安全性依賴持久層 fencing。 |
| `context_impl.go:2030`、`:2138` | 預設 acquisition 重試上限 5 分鐘、單次 IO timeout（註解預設 5 秒）；ownership lost/lifecycle cancel/停止狀態會終止。最終失敗呼叫 Stop，取消 lifecycle 並交 controller 關閉 engine。**這是 source 行為，本輪未注入 acquisition 失敗。** |
| `service/history/workflow/context.go:890`、`:408` | write error defer 清除 workflow context；之後重新載入 MS，避免把未提交的記憶體變更當成事實。 |
| `service/history/handler.go:2309` | `*persistence.TimeoutError` 在 History RPC 邊界轉成 `*serviceerror.DeadlineExceeded`。 |

[Source-confirmed] `common/persistence/client/factory.go:270` 的 persistence retryable wrapper 重試 Unavailable/DataLoss；而且 SQL 的 Unavailable 可來自 begin、query、rollback 前失敗或 commit 失敗。因此 **Unavailable 並非跨層一致的「已提交但未知」標誌**。本輪選用明確會進 default 分支的 `persistence.TimeoutError`，放在完整 production manager delegate 外層，避免低層重試改變一次性 fault 的定義。

[Source-confirmed] uncertain branch 與 `common/persistence/error_type.go:5` 的 `OperationPossiblySucceeded` 分類共同控制 recovery 與 task read watermark；不能只看 RPC status 推論底層 outcome。

## 3. Persistence commit、fence 與 authoritative read

| 位置 | Source-confirmed 行為與資格限制 |
|---|---|
| `common/persistence/sql/execution.go:334` | 先 AppendHistoryNodes，**再**執行 MS transaction；history append 單獨成功不構成有效 WFT acceptance。 |
| 同檔 `:40` | `txExecuteShardLocked` 在同一 transaction 先 `readLockShard(shard,range)`，才執行 MS mutation。 |
| `common/persistence/sql/execution_util.go:23`、`:630` | 檢查既有 DBRecordVersion／NextEventID，再寫 execution、durable tasks、ActivityInfo 等；本次以 DBRecordVersion 前一版條件保護 mutation。 |
| `common/persistence/sql/common.go:52` | fn 成功後才 tx.Commit；fn 失敗 rollback。Commit error 不代表必然 abort。 |
| `common/persistence/sql/shard.go:82`、`:124`、`:152` | UpdateShard 以 PreviousRangeID 檢查後更新 durable range；workflow transaction 的 range 不符回 ShardOwnershipLost。 |
| `common/persistence/sql/sqlplugin/sqlite/shard.go:18`、`:72`、`:86` | SQLite 的 read/write lock helper 是 transaction 內 SELECT，並非 PostgreSQL 式 row `FOR UPDATE`；不可把 helper 名稱當成所有 backend 的相同鎖語意。 |
| `common/persistence/sql/sqlplugin/sqlite/db.go:59` | 使用實際 SQLite transaction BeginTxx/Commit/Rollback。 |

[Source-confirmed：外部文件] SQLite 說明跨 connection 的隔離、單寫者與 WAL snapshot；舊 snapshot 嘗試升級寫入可能收到 SQLITE_BUSY_SNAPSHOT。shared-cache 若另啟用 read_uncommitted 有例外。本測試配置沒有設定 read_uncommitted，但沒有額外執行 PRAGMA 稽核或 crash 測試。參考：[SQLite Isolation](https://www.sqlite.org/isolation.html)。

[Model-inferred] 對舊 range r 的 MS/task transaction，權威屏障需要下列條件組合，而非只有「看到 range log」：

1. 新 range r′>r 的條件 transaction 確實提交。
2. 底層 transaction isolation / range check 保證舊 range 寫入要麼已在屏障前提交，要麼不能在屏障後產生新的有效 MS/task acceptance。
3. shard 已恢復 Acquired，失敗操作的 speculative MS 已清除；後續 read 為新的持久層讀取而非舊 transaction snapshot 或錯誤 cache。
4. 讀取結果以 persisted MS 的 branch、NextEventID、版本與 pending WFT 為準；不能用任意 appended history rows 代替。

[Unknown] 本輪沒有在 Acquiring gate 期間另外送新業務 RPC，亦未送出一筆 stale-range SQL write；「新操作被拒／延後」與「舊 range 被 fence」主要是 source 義務，不是本輪逐一觀測的事件。A/B 抵達 renewal 時沒有尚未返回的目標 manager 呼叫，沒有用這兩次 drain 成功宣稱驗證一般在途排空。

[Unknown] 本輪未保留任何底層 SQL transaction 持續在途，因此第 2 點的競賽沒有 runtime 證明。甚至 range fence 也不保證「任何 old request 都不能 append 物理 history row」：append 在 range-conditioned MS transaction 之前。原始 row 與有效 branch/range 必須分開看。

[Runtime-supported + Source-confirmed] A/B 均捕捉到同一 shard ContextImpl 的 UpdateWorkflowExecution stack、真實 renewRangeLocked/acquireShard stack、durable shard row=2、以及經 `ContextImpl.GetWorkflowExecution → LoadMutableState` 的新讀取 stack。測試 `phase=2` 僅是 **hook 已觀測 range delegate 成功**的標記，不是存取 private shard state；Acquired 的判定另外依實際 GetWorkflowExecution 已通過 source 的 state guard。沒有把 phase 變數或 log 自己當成權威狀態。

## 4. Hook provenance 與可行性

[Source-confirmed] 使用既有 `temporal/server_option.go:185` 的 `WithPersistenceFactoryProvider`，由 `TestClusterConfig.AdditionalServerOptions` 注入；`tests/testcore/onebox.go:175` 已有同類 production-factory wrapping 用例。

- A：只攔截目標 workflow 下一次 UpdateWorkflowExecution；先呼叫完整真實 ExecutionManager（包含 SQL store），要求成功返回後，向 shard caller 回相同 TimeoutError。
- B：同一攔截層、同一錯誤型別與文字，略過該次 delegate。因此 B 是 before-delegate no-commit outcome，不是已開始 SQL transaction 後 rollback 的實驗。
- 真實 ShardManager.UpdateShard 在 recovery 時用 channel gate 暫停；讀取獨立資料，再放行真實 delegate；未改寫 RangeID/MS/task/history row。
- 所有 task deletion 呼叫僅等待 gate 後呼叫真實 delegate（context 結束則回真實 cancellation），沒有以假成功刪除或造出 row。Activity dispatch 另用既有 HistoryTransferTaskInterceptor gate。
- 本輪不啟用 HistoryTaskRecorder；證據不以 recorder task IDs 決定查詢範圍。
- 既有 `common/persistence/faultinjection/fault.go:43` 也有 ExecuteAndTimeout 的 post-delegate 概念；本輪採 factory seam 取得每 workflow、一次性、確定性控制與 range 觀測。

[Runtime-supported + Source-confirmed] 注入從真實 normal WFT handler stack 抵達 shard UpdateWorkflowExecution；該函數 652–655 行在 wrapper 返回後無條件完成 tracker 並呼叫 handleWriteError。真實 acquireShard 的 stack 來自 transition goroutine，且原／新 durable range 交叉驗證。這證立 uncertain-write handler 路徑確實被 exercise，並非僅替換 frontend 最終 response。

[Unknown] Optional C 未執行。選定 seam 能控制 delegate 前後，尚未找到經本輪驗證、可在 SQL transaction 內保持真實 old-range write 在途且不改 production 的 hook。不能以「delegate 尚未開始」偽稱已控制底層 in-flight race。這不是對所有可能測試方法的不可能性主張。

## 5. 實作前預登錄與有效 fixture

[Source-confirmed] `preregistration.json` 時間 `2026-10-23T13:31:09.625181+00:00`，當時 test source 尚不存在；原始預測 SHA-256：`2684d18184f7657c9baa3957da40b2673fe699d2ca25b722e24656bd8d115187`。預測文字未在看到結果後改寫。

[Model-inferred → 本輪 Runtime-supported] Control 一次接受；A commit 後未知錯誤、range recovery、不再次接受；B 未提交、相同錯誤／recovery、pending K 維持，之後正常接受一次。預期 RPC error 是 DeadlineExceeded；此 status 不用來判斷 commit。兩個 test harness 範圍參數錯誤不屬模型 falsification。

[Runtime-supported] 每個 arm 都由真實 Server StartWorkflow/PollWFT 建立 history，不是手寫 history fixture：

- 初始有效 history：1 WorkflowExecutionStarted、2 WorkflowTaskScheduled、3 WorkflowTaskStarted。
- K=(namespace UUID, workflowID, runID, scheduled2, started3, attempt1)；120 秒正常 WFT，沒有 NextPageToken。
- 單一命令 ScheduleActivityTask，ActivityID=`ccpo-activity-1`，input=`"ccpo-input"`，schedule-to-close 300 秒、start-to-close 60 秒、eager=false。
- 成功後有效 history 追加 4 WorkflowTaskCompleted（references2/3）、5 ActivityTaskScheduled（reference Completed4）。
- A/B 重試是 **各 arm 內 byte-identical** 原始 token/commands/request。三個 arm 使用不同由 Server 產生的 namespace/run/time，並不是同一資料庫狀態或同一 complete Worker observation。
- token 中 WFT identity 與 shard RangeID authority 不是同一概念：B 原 token 在 range 2 仍可完成原 pending K。

| run | `request.pb` SHA-256 |
|---|---|
| Control | `55d2ea2cc0fdea2aa13bd5d4eb13f9602d2c21ebe1cc045e97e4f2044a2831ff` |
| A | `3a207dbb23c030402d03803ddb90240372ab6d218bcb5050e07934392bce4c3c` |
| B | `ec324cb5c14037e1e6107ad33b3f334a3831fc80e82bcd93569c7ee1139a969a` |

[Runtime-supported] 每次 request、token、各觀測階段 MS 與有效 history 均保存 deterministic protobuf，完整 hashes 見 SHA256SUMS 與各 run 的 protobuf-artifact 記錄；動態 UUID/timestamp 不要求跨重新執行 byte equality。

## 6. 執行記錄與結果

共同命令：`GOMAXPROCS=2 go test -p 2 -tags test_dep -count=1 -timeout 3m -run '^TEST_NAME$' -v ./tests/ccpo_c4`。runner 另外設定 CCPO_EVIDENCE_OUTPUT；逐次 cwd、UTC、source/test hash、stdout/stderr、exit code 完整保存。

| 記錄 | Test | Exit | 分類 |
|---|---|---:|---|
| run-01-control | TestCCPOC4Control | 1 | INCONCLUSIVE：列舉 immediate task 上界不應含 scheduled FireTime；尚未送 completion。 |
| run-02-control | TestCCPOC4Control | 1 | INCONCLUSIVE：列舉 scheduled task 上界 TaskID 應為零；尚未送 completion。 |
| run-03-control | TestCCPOC4Control | 0 | CONFIRMED SAFETY |
| run-04-committed-unknown | TestCCPOC4CommittedUnknown | 0 | CONFIRMED SAFETY |
| run-05-not-committed-unknown | TestCCPOC4NotCommittedUnknown | 0 | CONFIRMED SAFETY |
| lint-check | make lint-code-fast GOLANGCI_LINT_BASE_REV=HEAD | 2 | 工具準備失敗；離線模式的 toolchain checksum 驗證阻擋 golangci-lint 安裝。**未完成 lint，不是 lint PASS。** |

[Runtime-supported] 最終三個有效 run 使用完全相同 test SHA。最初兩個失敗版本與原始紀錄完整保留；修正只涉及 `validateTaskRange` 的類別 API 使用。

| 階段 | Control | A：committed unknown | B：not committed unknown |
|---|---|---|---|
| 初始 | range1, rev3, Next4, pending2/3 | 同左 | 同左 |
| 故障 delegate | 正常成功 | 真實 delegate 成功 | delegate 未執行 |
| 首次 caller 結果 | success | DeadlineExceeded | DeadlineExceeded |
| recovery gate 尚未放行時獨立讀取 | 不適用 | range1, rev4, Next6, 完成1/Activity1 | range1, rev3, Next4, pending2/3 |
| durable range barrier | range1 不變 | **1→2** | **1→2** |
| barrier 後 | rev4, completed | rev4, completed | rev3, pending2/3（與故障前 MS 完全相同） |
| byte-identical retry | 不需要 | NotFound；rev4／MS／task集合不變 | success；rev3→4，Next4→6 |
| 最終有效 Completed(K) | 1 | 1 | 1 |
| 最終有效 ActivityScheduled | 1 | 1 | 1 |
| recovery | 可 PollActivity event5 | 可 PollActivity event5 | 可 PollActivity event5 |

[Runtime-supported] A/B 實際時序均為：write range1 → 注入 Timeout → 真實 renewal entered(1,2) → caller 未收成功 → gate 內 privileged read → release → UpdateShard delegate 成功 → 獨立 durable range2 → History 實際 MS reload → retry 決策 → Activity dispatch。時序由 channel 與 source call stack 建立；沒有用 elapsed sleep 建立 ordering。

[Runtime-supported] A 的新 MS load 由實際 History queue 路徑先發生，B 的 load stack 直接包含 RespondWorkflowTaskCompleted retry。**不把 A 的 load 錯稱為一定由第二個 RPC 首先觸發**；兩者都顯示原錯誤 cache 已清除，barrier 後經 guarded persistence path 重建 state。

[Runtime-supported] MS transition count：初始2、接受後3；A retry 不再增加。有效 read 範圍皆取 persisted current version-history branch 與 `[1, NextEventID)`，完成前到3、完成後到5；沒有用 append 是否存在推定 acceptance。

## 7. 獨立 durable task range 證據

[Runtime-supported] 讀取的是 test cluster 獨立 ExecutionManager，而非 fault wrapper 的回應／recorder。列舉六個持久化類別：transfer、timer、visibility、replication、archival、outbound；memory-timer 非本輪 durable category。

- immediate：TaskID `[0, 9223372036854775807)`，FireTime 為 epoch。
- scheduled：FireTime `[1970-01-01T00:00:00Z, 2262-04-11T23:47:16.854775807Z)`，TaskID bound=0。
- batch size=2，追蹤每個 NextPageToken 直到空；有資料的主要類別確實跨頁。
- 每次所有 row 都比對 namespace/workflow/run；本次沒有其他 workflow row，沒有 duplicate keys。
- 初始數量 transfer1/timer2/visibility1；成功後 transfer2/timer3/visibility2，另三類均0。
- 既有 WFT transfer／timeout row 因 cleanup gate 暫留，不代表該 WFT 還 pending；pending 以 MS 判定。

| run | 新 Activity transfer ID | 新 Activity timeout ID | 新 visibility ID |
|---|---:|---:|---:|
| Control | 1048591 | 1048592 | 1048590 |
| A | 1048591 | 1048592 | 1048590 |
| B（range2 retry） | 2097156 | 2097157 | 2097155 |

[Runtime-supported] A barrier 後到 retry 後，六類列舉結果完全相同；B barrier 後到 retry 前，六類結果與初始相同。每個成功 arm 只新增上述三筆。`audit-observations.py` 可從原始 log 重算並驗證所有 protobuf hashes，輸出為 `independent-observation-audit.json`。

[限制] 這比 C-3 recorder-based known-ID discovery 完整，範圍不由 recorder 決定。但不是跨六類別的一個 SQL snapshot，也不涵蓋所有歷史時刻。觀察窗內同一 workflow 沒有其他 command mutation、Activity dispatch 被擋、WFT timer 尚未到期、cleanup 被延後；這些是穩定列舉的 harness 條件。解除 gates 後只證明 Activity 可交付，未再次全域列舉後續所有任務。

## 8. 各 safety / progress obligation 的實際結論

| Obligation | 結論 |
|---|---|
| Commit interpretation | Source-confirmed + Runtime-supported：以 MS revision、有效 branch/range、WFT causal refs、ActivityInfo、durable tasks 判斷，沒有以 append 單獨判斷。未注入「history-only append 成功但 MS 未提交」分支。 |
| At-most-one acceptance | CONFIRMED SAFETY：對各自 K，受控 A/B 與 Control 的有效 Completed/ActivityScheduled 各1；A 重試無第二次 revision/task。 |
| Range fencing | Source-confirmed range-conditioned SQL 路徑與 Runtime-supported 真實 1→2 renewal；**未知 old in-flight SQL 在新 barrier 後的競賽仍 UNKNOWN**。 |
| Recoverable obligation | CONFIRMED SAFETY：A committed work 留存並可 dispatch；B 不被假標完成，pending K 可重試。 |
| Conditional progress | Runtime-supported 有效 recovery witness：reacquisition 成功及服務可用時，兩 arm 最後都 PollActivity 成功；未驗證無限失敗、失效 K 的 timeout successor 或完整 workflow termination。 |
| Failed reacquisition shutdown | Source-confirmed；本輪 runtime UNKNOWN。 |
| External effect uniqueness | UNKNOWN：只有 durable schedule 與 PollActivity，沒有外部 Activity effect execution。 |

## 9. 相較 C-3 的新增證據與限制

[Runtime-supported] C-3 的 post-handler response suppression 不會令已成功的 shard write 進入 uncertain handling，且 precommit ResourceExhausted 是明確 noncommit 類別。本輪在 manager→shard 邊界回 Timeout，實際執行 Lost/Acquiring/reacquire、更新 durable range、cache reload 與 retry，並改為獨立完整 task-range 列舉。這是新增的 recovery witness。

[Unknown] 仍不涵蓋：物理 database commit-timeout、SQL transaction 中斷、crash/power-loss、跨 owner／跨 process 接手、multi-cluster、Cassandra/PostgreSQL、acquisition 長期失敗、舊 range 在途 SQL race、speculative/paginated WFT、eager、Continue-As-New、Reset、external Activity exactly-once。SQLite WAL synchronous=normal 不能拿本次非 crash 觀測推出 power-loss durability。

[Unknown] 沒有證明 A/B 在 Worker 所有可用觀察上完全不可區分。本測試 raw client 的第一次 error 型別／文字相同，但 namespace/run/time、後續 recovery 與 privileged harness observations 不同。這不是 C-1 的完整 observation-equivalence runtime 證明。

## 10. 對 CCPO belief-state／authority 模型的影響

[Model-inferred] 應分開表示：logical WFT K、RPC attempt、已提出 mutation、actual commit outcome、caller error observation、shard authority epoch r、context phase、MS DB revision、cache freshness、effective history boundary、durable task obligations。

[Model-inferred] 未知 error 的 worker belief 仍可包含 committed / not-committed；此時「讀到未完成」若沒有 barrier，可能與晚到的舊寫入衝突。`AuthorityCertificate(r′, read)` 至少要求：真實 range 條件更新已提交、fencing contract、Acquired admission、fresh MS read。**新 epoch 本身不是 operation-success receipt**：A/B 都升至2，只有 MS read 區分 completion/pending。

[Model-inferred + Runtime-supported witness] 安全 retry 不要求 worker 先得知原始 commit outcome。相同 token 的 server-side validation，在 A 讀到完成後拒絕、在 B 讀到 pending 後接受一次；配合持久化條件檢查與 recovery barrier，可讓同一 retry action 對兩種 outcome 都安全。這是「協定使兩個可能世界接受同一安全動作」的受控例子，不是從模糊 timeout 直接推出 dedup 或成功。

[Model-inferred] 建議 CCPO 加入獨立義務：

1. uncertainty 分類必須標明 layer 與 operation；不能只保存 error code。
2. request drain 與 physical transaction resolution 必須分開。
3. authority evidence 的產生、scope、失效與 read freshness 明列；不得拿 epoch 冒充 outcome。
4. 接受不變式限定 effective durable state，external effect ledger 另證。
5. safety 與 eventual reacquisition / service availability 的 progress assumptions 分開。

[Unknown] 要把本輪 witness 提升成一般 fencing theorem，還須證明／測試真正 delayed old transaction、authority transfer、crash 與各 persistence backend 的讀寫隔離契約。**本輪停止於 C-4，不啟動下一階段。**
