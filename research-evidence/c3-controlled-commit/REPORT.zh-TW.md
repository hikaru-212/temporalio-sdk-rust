# Temporal CCPO C-3：normal WFT commit ambiguity

**結論：本次限定情境為 CONFIRMED SAFETY；沒有 correctness RED。** 真實 Server／檔案型 SQLite 在成功回應被抑制、明確提交前失敗及同一 K 並行重送下，均未重複接受 normal WFT 的 ScheduleActivityTask batch。已提交工作可由正常 Activity polling 取回。這不等於外部 Activity exactly-once、所有 persistence backend 安全證明或 crash recovery 驗證。

證據標籤：Source-confirmed＝固定 SHA 原始碼；Runtime-supported＝本次實際執行及直接 persistence 讀取；Model-inferred＝有限證據下的模型推論；Unknown＝未測或未證明。

## 1. C-2 checkpoint 已先完成

- 分支：`research/ccpo-egts-c2-temporal-commit`。
- commit：`70acd8a9eaad6a14a0b58ccf113c0cfda9e4cad1`。
- parent：`029a790e4fa7663cf70c8fedc82b379d47f99dbb`。
- 已核對 repository owner／push URL，只正常推送至 `https://github.com/hikaru-212/temporalio-sdk-rust.git`；遠端 branch SHA 已確認等於本地 commit。
- 原始 C-2 報告從既存 Codex session 的 assistant 訊息擷取，256 行，明確標記為 conversation reconstruction，沒有偽裝為既有獨立檔案。公開版只轉換 33 個本機來源連結為固定 SHA permalink，正文與結論不變；未改寫原調查為 runtime 結果。
- 原文 SHA-256：`345d9eb218f38dd4aa895b321f5aa61bb1852c14cfdb8a9ad0ab5a3429bf294d`；公開稿：`fea32ce4662be18509f2d3983f25d0b46cd27b719b82e1b0b721de1c3fcd77e6`。
- 六個新增文件均在 `research-evidence/c2-temporal-commit/`：README、REPORT.reconstructed.zh-TW.md、SHA256SUMS、preservation-audit.json、provenance.json、publication-links.json。
- 既有 C2-D／C2-E2／C-1 integrity entries 分別 47／96／4 項全部吻合。C-2 clone 與原 Server checkout 最終均 clean。沒有 PR、issue、comment 或 upstream write。

C-3 在上述 checkpoint 成功推送、核對後才開始；本報告及所有 C-3 文件**未 commit、未 push**。

## 2. 實驗基線與 harness 有效性

**[Source-confirmed / Runtime-supported]** Server `0d5e3848bed625907987f73d5eaae8dc77bc6590`；SDK-Rust 參照版本 `8f4a676f9e1a6276b5cc2f77e25c9183ce256fe9`，本次未啟動 Rust Worker。獨立 Server clone 為 detached HEAD，沒有修改原 Server／SDK worktree。唯一新 Go 檔為 `tests/ccpo_c3/commit_ambiguity_test.go`，309 行。

**[Source-confirmed]** 使用既有 `testcore.NewTestClusterFactory`、真實 frontend/history/matching services 與 `GetSQLiteFileTestClusterOption`。SQLite 設定 WAL、synchronous=normal；不是 mock store，也不是手寫狀態機。以真實 StartWorkflowExecution/PollWorkflowTaskQueue 取得 Server 生成的 normal WFT token，不使用合成 history。每個 arm 使用獨立資料庫、namespace、workflow/run。

**[Runtime-supported]** 初始 history 為 Started1、Scheduled2、Started3；Mutable State 的 `workflow_task_type=WORKFLOW_TASK_TYPE_NORMAL`、attempt=1、NextEventId=4、DBRecordVersion=3、StateTransitionCount=2、RangeID=1。WFT timeout=120 秒；各測試約 0.05–1.05 秒，直接讀取證實未發生 timeout 或 token 失效。

**[Source-confirmed / Runtime-supported]** 完成請求只含一個 ScheduleActivityTask：ActivityId=`ccpo-activity-1`、type=`ccpo-activity`、input=`ccpo-input`、eager=false。沒有 speculative／completion pagination、CAN、Reset、failover、外部 Activity executor。每次重送使用相同 serialized token、commands、identity；呼叫前比對 deterministic protobuf bytes。`Identity=ccpo-client` 是 worker 標籤，不當成去重 key；本請求沒有另加虛構的 request-id 欄位。

**[Source-confirmed]** A 使用既有 frontend post-handler response callback；只在真實 handler 成功返回後，把成功 response 換成 Unavailable。這是成功回應抑制，不是實際 socket packet-drop。B 使用 `faultInjectionExecutionStore.UpdateWorkflowExecution` 的既有 runtime injector，在 delegate SQL store 被呼叫前回傳 system-scoped ResourceExhausted，保持注入直到第一個外部 RPC 返回。B 因此也早於 SQL history append，**沒有測「history 已 append、Mutable State 尚未 commit」**。

**[Source-confirmed / Runtime-supported]** 既有 namespace-scoped `HistoryTransferTaskInterceptor` 只以 channel 暫停目標 Activity transfer task 執行；不改 completion、驗證或 persistence 語意。利用這個屏障直接讀出 task row，之後解除並經正常 matching/poll 路徑取回 Activity。沒有用 elapsed sleep 決定操作順序。

## 3. 預登錄與執行紀錄

`PREDICTIONS.md` 在 `2026-10-23T12:07:05.792640Z` 登錄，早於首次執行；SHA-256 `7e5bbcd0344551bcb0254a37d764ee4c35cf64c43c751977cbfe33e8cee3fcbf`。

預測：Control 一次接受；A 提交後丟失成功回應，原請求重送不能再接受；B 明確提交前失敗且 K 仍有效時，原請求重試可一次接受；三者均需直接 DB 證據及後續可取回工作。並行 arm 預測兩請求只能一次接受，且僅在主要 A/B 通過後執行。

每次命令為 `GOMAXPROCS=2 go test -p 2 -tags test_dep -count=1 -timeout 3m -run '^TEST_NAME$' -v ./tests/ccpo_c3`。各 run 的 `command.json`、`result.json` 保存完整 argv、來源 SHA、測試 SHA、開始/結束 UTC 與 exit code。

| Arm / test name | 原始 run | 補全 task ledger 的 run | exit codes |
|---|---|---|---|
| Control / TestCCPOC3Control | run-01-control | run-05-control-ledger | 0 / 0 |
| A / TestCCPOC3ResponseLost | run-02-response-lost | run-06-response-lost-ledger | 0 / 0 |
| B / TestCCPOC3PrecommitFailure | run-03-precommit | run-07-precommit-ledger | 0 / 0 |
| Concurrent / TestCCPOC3Concurrent | run-04-concurrent | run-08-concurrent-ledger | 0 / 0 |

**[Runtime-supported]** 八次皆 PASS。第二組只補記所有成功 task-write 類型，沒有修改請求、故障位置、預測或判準；變更原因保存在 `evidence-extension.json`，第一版測試及全部原輸出均保留。每次 `stderr.log` 為空；原始 stdout 含完整 Server log。B 的 error-level log 是已注入的 pre-store rejection，不能當成 correctness RED。沒有編譯失敗或測試 harness failure。

獨立的 repository lint 嘗試 exit=2：cached-only 設定下無法校驗 Go toolchain，且缺少 lint 工具；lint 未啟動。詳見 `lint-check/`，不宣稱 lint PASS，也不把它改分類為 runtime correctness failure。

## 4. Control／A／B 時序及 durable acceptance

下表採補全 ledger 的 run-05～08。所有有效 history 都由**直接讀到的 Mutable State branch 與 NextEventId 上界**決定，沒有把未被 Mutable State 納入的 raw append 當作接受。

| Arm | 第一／第二完成結果 | DB revision | 有效 Completed / ActivityScheduled | Pending WFT | Activity transfer task | Recovery |
|---|---|---|---|---|---|---|
| Control | success／不需第二次 | 3→4 | 1 / 1 | K→無 | 1048591，一筆 | poll 得到 Activity1 |
| A | injected Unavailable／NotFound | 3→4→4 | 1 / 1，重試前後相同 | 無 | 1048591，同一筆 | poll 得到 Activity1 |
| B | ResourceExhausted／success | 3→3→4 | 0 / 0→1 / 1 | K 保持有效→無 | 失敗後無；成功後 1048602 | poll 得到 Activity1 |
| Concurrent | 一個 success、一個 NotFound | 3→4 | 1 / 1 | K→無 | 1048591，一筆 | poll 得到 Activity1 |

**[Runtime-supported]** 最終有效 history 都恰為 events 1–5：WorkflowExecutionStarted1、WorkflowTaskScheduled2、WorkflowTaskStarted3、WorkflowTaskCompleted4、ActivityTaskScheduled5。Completed4 引用 Scheduled2／Started3；ActivityScheduled5 引用 Completed4。MS NextEventId=6、StateTransitionCount=3、ActivityInfos 只有 key=5、activity_id=`ccpo-activity-1`。各次觀察 RangeID 皆 1，沒有 ownership change。

**A 時序 [Runtime-supported]** 真實 handler success → callback 記錄並抑制成功回應 → client 收到 Unavailable → 直接讀出 revision4／有效 Completed4／ActivityScheduled5／Activity task row → 重送完全相同請求 → NotFound → 再讀 MS protobuf 與 DBRecordVersion，均完全相同 → release → 正常 poll 取回 Activity。第一次成功的證據來自 DB，並非 callback 的成功值或第二次 NotFound。

**B 時序 [Runtime-supported]** 初始 revision3 → handler 生成 proposed mutation → 第一次 pre-store rejection → Server 內部重試，第二次 pre-store rejection → client 收到 ResourceExhausted → 直接讀出 revision3，MS protobuf 與故障前完全相同、沒有有效 Completed／ActivityScheduled／pending Activity → 關閉注入 → 重送原請求 → success／revision4 → release／poll 成功。

B 原始與補全 ledger 執行都各記錄 **2 次**被拒絕的 persistence 呼叫。兩次都提出 revision4／NextEventId6，但並未提交。它們分配的 task IDs 不是 durable acceptance，不能計為重複已接受的 commands。沒有 K timeout、重新發 task 或 RangeID 變動造成混淆。

**Concurrent [Runtime-supported]** 相同初始 K、同一 request bytes，由共同 channel 同時放行兩個 goroutine；補全 ledger run 的 concurrent-1 成功、concurrent-0 NotFound。只驗證此排程下兩個呼叫不能雙重接受；未證明兩個 SQL transactions 同時進入 CAS，也未故意移除 Server workflow lock。

## 5. Token、tasks 與 recovery 證據

**[Runtime-supported]** K 包括 namespace UUID、workflow ID、run ID、Scheduled2、Started3、attempt1、started_time、version 與 shard vector clock。原始 token hex、decoded protobuf、完整 completion request 與 hash 都在每次 stdout 的 `request` 記錄。Version 為 protobuf default 0，JSON 省略該欄，不表示未定義。

| 補全 run | request SHA-256 | 新增 successful task-write records：transfer / timer / visibility |
|---|---|---|
| Control | c0f111871816fcaf8253ea80bb3073e2334f18ee6dd478387fd5c1e0be9619c2 | 1048591 / 1048592 / 1048590 |
| A | 267efca2e57c153a9921f809e2702ac3f9857bab47d6bcac053f027fa8f263cf | 1048591 / 1048592 / 1048590 |
| B | a060cb5fdad15072e2b9300b859e68b16e8c202a9e47fd7e79d0c4aa99e694fe | 1048602 / 1048603 / 1048604 |
| Concurrent | 315cf7af22f3639652b224d2a116e6b4954fd1fd8c6c5c3c326ab8e4895a2ab7 | 1048591 / 1048592 / 1048590 |

三類分別是 `TransferActivityTask`、`ActivityTimeout`、`VisibilityUpsertExecution`，每類新增一筆。完整 cumulative ledger 也保留初始 workflow timer、WFT timeout、WFT transfer、visibility start。每個 arm 的 UUID、時刻不同，跨 arm request hash 不需相同；**同 arm 的 retry bytes 必須相同，已由測試斷言**。

**[Runtime-supported]** Activity transfer row 另經真正 `ExecutionManager.GetHistoryTasks` 直接讀取；完整 task ledger 則由既有 recorder 在 delegate persistence 成功後記錄。不要混稱所有 timer／visibility rows 都經本次獨立逐列讀取。有效 history、MS 與 Activity row 的直接讀取提供獨立於 mock-success 的提交證據。

**[Runtime-supported]** 每個 arm 最後都從真實 matching 取回 ActivityId=`ccpo-activity-1`、scheduled_event_id=5、attempt1、相同 input、正確 run ID。未執行 Activity 函式、未回報 Activity completion。這是**存活 cluster 內的 durable work dispatchability**，不是 process restart／power-loss 復原，也不是 workflow 最終完成。

## 6. 原始碼對應與 CCPO 義務

以下全部 **[Source-confirmed]**，固定 Server SHA：

- `service/frontend/workflow_handler.go:1220`、`service/history/api/respondworkflowtaskcompleted/api.go:112`：completion entry 與實際 handler。
- `api.go:203`：當前 WFT／started ID、started time、attempt、version 驗證；已完成 K 不能再次進入同一 command application。
- `api.go:675`：normal completion 要求持久化；失敗時取消 effects，錯誤沿真實路徑回傳。
- `common/persistence/sql/execution.go:334`：先 append history，再進入 shard-checked MS transaction；`common/persistence/sql/common.go:52` 的 `tx.Commit()` 是本次 SQL transaction 成功邊界。
- `common/persistence/faultinjection/execution_store_gen.go:251`、`fault.go:61`：B 在 delegate operation 前注入，不可誤稱 SQL commit 中途失敗。
- `common/rpc/grpcfaults/interceptor.go:24`：A 的 post-handler response seam。
- `service/history/shard/context_impl.go:1522`：ResourceExhausted 是 definite noncommit 類別；unknown-write RangeID reacquisition 分支本次沒執行。
- `common/resource/fx.go:328`、`common/util.go:179`、`common/dynamicconfig/constants.go:160`：system-scoped ResourceExhausted 的 inter-service retry；與 B 實際兩次拒絕相符。
- `tests/testcore/history_task_recorder.go:83`、`service/history/transfer_queue_active_task_executor.go:108`：成功寫入記錄與 transfer 執行屏障。

**[Model-inferred]** 本例支持以下局部組合：K 的完成權限驗證＋成功提交後消耗 pending WFT＋有效 MS/history/tasks 的一致關聯，使「重送相同 normal WFT 完成」在 committed／not-committed 兩種情況都可維持 `accepted(K) ≤ 1`。重試不必先讓 client 精確知道第一次是否已提交，才能在本次情境安全；這是 protocol-tolerated ambiguity 的受控 witness。

**[Model-inferred / Unknown]** 本實驗沒有令 A/B 的完整 client observation 相等：A 是 Unavailable，B 是 ResourceExhausted，注入訊息也明示測試原因。因此不能聲稱已 runtime 證明 C-1 的完整不可區分世界，或從本次 status 例子推出歷史 commit outcome 可由 status 普遍辨識。尤其 NotFound 本身不識別先前是哪一種結果。

**[Unknown]** 一般安全命題仍需證明所有合法狀態的 token/ownership invariants、MS 與 task transaction 一致性、有效 branch 範圍、未知 persistence 結果與 fencing、queue crash/retry，以及指定 fairness 下的進展。本次只有有限 schedule、單 shard、單 cluster 與一種 command。

## 7. 分類、限制與保存

- Control／A／B／Concurrent：**CONFIRMED SAFETY，限上述 normal WFT 與存活 cluster recovery scope**。沒有 duplicate acceptance 或 first correctness divergence。
- 全部 runtime tests PASS／exit0；lint 工具準備失敗，獨立記為 INCONCLUSIVE tooling check。
- Unknown：Cassandra/MySQL/PostgreSQL、SQLite power loss、commit 途中 timeout、history-only append、Shard RangeID change、failover、token expiry、SDK-Rust 自動 retry/eviction、speculative/paginated completion、外部 Activity exactly-once。未主張一般 semantic refinement。
- SQLite 使用實際檔案與 transaction，但 synchronous=normal；未保存 shutdown 後已由測試設施清除的 DB 檔。保存的是提交期間直接讀取的完整 MS/history/task 證據與原始 log，不偽稱可供後續離線重開的 DB image。
- 所有 namespace/workflow/activity/client identity 均為本測試專用合成名稱或 Server 生成的測試 UUID，沒有業務資料或真實 worker token。原始 logs 包含本機路徑，僅本地保存；未公開 C-3。
- `test-only.diff`、兩版 test hash、八組 raw stdout/stderr、exit codes、完整 observations、預登錄、來源／安全 audit 與 SHA256SUMS 已保存。所有 `observations.json` 明列原 stdout hash 與行號，是衍生索引，不冒充原始輸出。
- 停在 C-3；沒有 C-4、production repair、C-3 commit/push 或 PR。
