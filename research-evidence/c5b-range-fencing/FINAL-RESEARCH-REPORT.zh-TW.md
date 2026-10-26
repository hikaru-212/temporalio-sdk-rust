CCPO C-5B 公開研究報告

**CONFIRMED SAFETY（受限範圍）**：[Runtime-supported + Model-inferred] 2026-10-26的Main、Positive、Negative各自完成所需SQL／authority ordering、獨立持久狀態讀取、合法續行與成功cleanup。每個arm的normal WFT identity K在所測有限trace中至多一次有效接受，最後均為一次。未發現correctness RED。本報告整理既有實驗，沒有新的execution。

[Source-confirmed] 固定Server SHA為`0d5e3848bed625907987f73d5eaae8dc77bc6590`。三組使用同一份P2後test-only source；Go1.27.0 linux/arm64、modernc.org/sqlite v1.51.0。詳細source SHA與original manifests SHA在PROVENANCE.json及SOURCE-SHA256SUMS。研究fork parent為`ea6507ed48614f71a7426740b61c51029f3c9541`。

| Arm | 原始執行開始 UTC | Test | Exit | SQLite／transaction outcome |
| --- | --- | --- | --- | --- |
| Main | 2026-10-26T11:17:31.069287+00:00 | TestCCPOC5B2Main | 0 | 真正mutation得到517，commit0、rollback1 |
| Positive | 2026-10-26T11:33:04.048899+00:00 | TestCCPOC5B2Positive | 0 | 真正Commit=nil，commit1、rollback0 |
| Negative | 2026-10-26T11:43:01.221769+00:00 | TestCCPOC5B2Negative | 0 | 實際range validation拒絕，commit0、rollback1 |

完整八項比較在[COMPARISON.zh-TW.md](COMPARISON.zh-TW.md)。三組分別建立不同DB／namespace／workflow；各自K包含run、Scheduled2、Started3、Attempt1。每個arm內的retry保持相同token／commands，實際RPC前驗證deterministic request bytes；不是三個arm共用同一個token。

[Runtime-supported] Main中，真正舊transaction讀r1後停在首個mutation前；controlled error使真實shard recovery更新r2。guarded fresh read觀察revision3與pending K；同一仍開啟transaction再probe仍讀r1，context未被提前取消。release後真正UpdateCurrentExecutions回`*sqlite.Error: database is locked (517)`，rollback成功。pre/post Mutable State、effective history及六類ledger完全相同。合法retry後revision4、唯一Completed4／ActivityScheduled5及ActivityInfo；Activity transfer task2097156指向event5。Activity poll成功。

Main事件定位：seq37 BeginTx、38 shard read1、39 gate、40 controlled error、44 renewal返回、47 guarded read、49同Tx舊snapshot probe、62 release、64真實mutation、65 raw error517、66 rollback、67 delegate terminated、80 post-state、83合法retry、96 retry後state。公開events同時保留原始stdout行號。517對應driver的SQLITE_BUSY_SNAPSHOT，並非由Unavailable推測。

[Runtime-supported] Positive中，old-range transaction先完成mutation及Commit（seq32），delegate done後才回controlled error（36）；真實renewal（41）後guarded read（43）觀察revision4及completed K。同request retry（58）回NotFound，但分類依據是獨立state/history/ledger在拒絕前後完全不變，並非NotFound本身證明原操作成功。唯一Activity transfer task1048591/event5，poll成功。

[Runtime-supported] Negative中，controlled error在實際delegate invocation前回傳（24）；背景goroutine可已存在但等待launch。真實renewal（28）與guarded fresh read（31）後才啟動原range1 request（45）。真實SQL shard read（48）看到2，Temporal產生`*persistence.ShardOwnershipLostError: Failed to lock shard. Previous range ID: 1; new range ID: 2`，在mutation gate前拒絕。rollback（49）成功，沒有mutation或commit。revision3／pending K／effective history／ledger保持不變；合法retry（64）後revision4、唯一接受及Activity transfer task2097156/event5，poll成功。Negative沒有SQLite mutation錯誤碼，不能套用Main的517。

[Runtime-supported] 三組poll後revision5是正常Activity start持久化，不是第二次WFT接受。沒有外部Activity implementation執行。所有有效history均依persisted Mutable State的current version-history branch與`[1,NextEventId)`讀取：未接受時events1–3、Next4；接受後events1–5、Next6。Completed4參照Scheduled2/Started3，ActivityScheduled5參照Completed4。physical history append成功本身不是有效接受。

| Arm | 研究events全部保留 | protobuf originals只保留hash | State snapshots | 六類enumerations | task pages |
| --- | --- | --- | --- | --- | --- |
| Main | 119 | 23 | 6 | 36 | 47 |
| Positive | 94 | 17 | 4 | 24 | 35 |
| Negative | 100 | 20 | 5 | 30 | 40 |

[Runtime-supported] 直接從persistence枚舉transfer、timer、visibility、replication、archival、outbound，BatchSize2、全相關range分頁至EOF；不用recorder-discovered IDs限制查詢。122頁原始研究觀察在公開投影中全部保留，page token用一致代號保留串接。State前後比較與task payload／identity一起判斷，不能用總row數替代因果驗證。

[Source-confirmed] Server `sqlExecutionStore.UpdateWorkflowExecution`先append history，再經`txExecuteShardLocked`與`readLockShard`進入MS transaction；range不符時由真實guard返回ownership error並rollback。Main／Positive gate位於實際LockCurrentExecutions讀取之後、第一個UpdateCurrentExecutions之前。pinned SQL實作與測試source可分開查驗；本包沒有production修改。

[Runtime-supported + Source-confirmed] 三組都確認delegate結果產生前context error=nil；接收done後才讀非atomic結果，SQL outcome與transaction closure有證據。physical connections在join／服務teardown之後關閉，registry entry歸零。全域modernc once dispatcher本身留至process exit，不能稱其卸載；Activity hook remove由cleanup source與成功teardown支持，沒有獨立removal probe。

[Source-confirmed + Model-inferred] tracker callback/drain及private shard Acquired狀態的部分次序由pinned source、真實stack與成功guarded read共同支持，沒有直接私有counter probe。RangeID2是authority/recovery certificate，不是原WFT commit receipt。

必須保留的限制：

- controlled ExecutionStore early error是manager/store outcome模擬，不是原生SQLite physical commit-timeout。Worker-facing DeadlineExceeded、manager TimeoutError、實際SQL結果是不同觀察。
- 僅normal already-started WFT、單SQLite環境及三個有限排程；不含speculative/paginated completions、crash、power loss、ownership transfer、multi-cluster或其他backends。
- WAL/private/read_uncommitted0有runtime觀察；設定另有synchronous=normal，未驗證fsync或power-loss durability。
- 多類task與MS/history讀取不是單一原子snapshot；受控task-delete／Activity-dispatch barriers穩定關鍵區間，不能推廣至任意並行queue狀態。
- poll後原assertions為revision≥4、Next≥6及remaining Activity transfer≤1；此三組實際讀到5／6／1，不把較弱assertion寫成一般exact equality。
- 成功cleanup路徑已觀察；P2 timeout/resource retention分支未做runtime fault測試，未跑race detector。
- 未完整枚舉physical orphan history rows；原臨時DB已依正常teardown移除，保留的是原始protobuf／log記錄，本公開包則提供去敏投影與hash。
- 不證明所有reachable schedules的fencing、不證明兩world完整Worker observations相同、不宣稱external exactly-once或一般Temporal correctness。

[Evidence provenance] Phase0／C-5B.1的歷史PASS仍屬部分恢復證據。新三組C-5B.2原始指定bundle完整保存，公開版刻意不含私有內容。未補造缺失logs、DB、protobuf；舊lint preparation failure仍是failure。原Main與Positive報告當時說尚未執行後續controls是正確歷史狀態，不能追改；本報告才是三組完成後的合併結論。

公開來源、轉換、限制與可驗證性見PUBLIC-TRANSFORMATIONS.json、PROVENANCE.json、SANITIZATION.zh-TW.md；原始hash可驗證保存承諾，不能單獨證明runtime觀察或取代私有原件的bytewise審閱。
