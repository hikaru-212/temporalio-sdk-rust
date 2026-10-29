# D-2：有限轉移系統與執行前規格

本檔在探索前保存。所有規則均為 **Model-defined**，不是新 Temporal source/runtime 結論。基礎是 D-1 第 7 節 P2；本輪只驗證兩項安全性，沒有新的 Temporal、SQLite 或 Server 實驗。

## 1. 範圍與狀態

固定一個已 Started 的 normal WFT 身分 K 與相同命令 payload p（象徵一個 ScheduleActivityTask）。兩個不同 attempt ID 為 0、1；K 不重用、沒有 Reset、timeout successor、跨 cluster、外部 Activity 執行或 crash。epoch 與 attempt ID、K、Mutable State revision 是不同變數。

全域狀態 `s=(r,u,P,v,H,E,Q,T0,T1,C)`：

| 變數 | 定義 |
|---|---|
| r | 已提交的 shard authority epoch，初始 1，獨立 renewal 可改為 2 |
| u | authority lifecycle：active / renewing / acquiring |
| P | K 的唯一 pending 資格；初始 true，成功接受後 false；沒有重建資格的規則 |
| v | 抽象持久 revision，初始 3；每次有效接受加 1；**沒有 4 的上限** |
| H | 已物理 append 的 attempt ID 集合；不表示有效接受 |
| E | 有效 command batch 接受的有序 ledger；每次提交 append attempt ID，初始空 |
| Q | 對應的 durable task 建立 ledger；初始空，不是以 K 去重的 set |
| Ti | stage、request epoch/revision、實讀 snapshot epoch/revision/pending、錯誤、manager result、caller observation、ack 狀態 |
| C | 尚未認證或 `(r,v,P,E,Q)` 的 immutable fresh-read certificate |

Ti stages：unused → submitted → appended → begun → read → mutated → committed；驗證失敗進 failed → aborted；入口看到已 consumed 則 request_rejected。`mutated` 僅是未提交 write set，沒有提前修改有效 D/Q。

manager/caller 值分別為 waiting / unknown / success / failure；ACK 為 none / sent / delivered / lost。未知結果不等於 abort，也不等於 commit。正常 ACK 的 sent、received 分離；受控 manager error 可以在背景 delegate 開始前、執行中或 commit 後發生。

初始值：`r=1,u=active,P=true,v=3,H=∅,E=Q=(),T0=T1=unused,C=⊥`。先命名第一個 attempt 為 0，之後可並行或重試 1；這是名稱約定，沒有只允許序列執行。

## 2. 完整轉移規則

每列為一個原子模型動作，scheduler 可在任意相鄰動作間切換。表中未列的欄位保持不變。

| 動作 | guard / 效果 |
|---|---|
| Submit(i) | Ti unused 且 u=active；i=1 時 0 已提交。擷取 request `(r,v,K,p)`。P=true 則 submitted，否則 request_rejected。可在第一個回覆之前並行提交，也可在 unknown 後重試 |
| AppendPhysical(i) | submitted → appended，`H←H∪{i}`；不改 P/v/E/Q |
| Begin(i) | appended → begun；**BEGIN 不建立 read snapshot** |
| ReadSnapshot(i) | begun 時實讀當前 r/v/P。request epoch 不等 r 則 range mismatch；P=false 或 request revision 不等 v 則 claim mismatch；否則 read。這些入口驗證在兩個 ablation 中皆保留 |
| TryMutation(i) | read；若 E-contract 啟用且 snapshot epoch≠current r，failed；否則 mutated。前者抽象表示 real write admission 的 read-set conflict |
| CommitRejected(i) | mutated；E-contract 啟用且 snapshot epoch≠r，或 T-contract 啟用且 `¬P ∨ snapshot revision≠v`，則 failed，沒有有效修改 |
| Commit(i) | mutated 且上述啟用的驗證皆通過。**單一原子動作** `P←false,v←v+1,E←E·i,Q←Q·i,stage←committed`。無 `AcceptCount<1` guard |
| Abort(i) | failed → aborted，保留錯誤及可能的物理 append；沒有有效修改 |
| ManagerUnknown(i) | 已 submitted 的非入口拒絕 attempt、manager waiting；manager/caller←unknown。不取消或終結 delegate |
| ManagerResult(i) | committed/aborted/request_rejected 且 manager waiting；manager←success/failure，caller 尚未收到結果 |
| SendAck(i) | manager 有確定結果且 ack=none → sent |
| DeliverAck(i) | sent → delivered，caller←manager result；可在 caller timeout 後遲到 |
| LoseAck(i) | sent → lost；不更改 durable state |
| ObserveTimeout(i) | attempt 已使用且 caller=waiting → unknown；不更改 durable state/transaction |
| RenewBegin | r=1,u=active → renewing |
| RenewCommit | r=1,u=renewing → r=2,u=acquiring；只改 authority row，不把 K 判為 committed |
| Acquire | r=2,u=acquiring → active |
| CertifyFresh | r=2,u=active,C=⊥；原子讀取當前 committed `(r,v,P,E,Q)` 保存為 C。**不要求舊交易關閉**，不以 P2(b) 作 guard |

renewal 可由環境獨立觸發；不要求先看見 manager error。這允許更多不利排程。有效 revision 採全域原子 `v+1`，不是逐 byte 模擬 SQL request 的賦值方式。弱化後同 K 接受兩次可到 v=5；此 v=5 不代表既有 C-5B 的 Activity polling revision5。有效 history 是 batch/frontier 的抽象，不模擬 event-ID 衝突或實體 SQL row uniqueness；那些額外限制若存在，可能另行阻止弱模型反例。

## 3. 契約的獨立開關

- **T-contract / atomic_claim**：成功 commit 前在同一原子步驗證 live pending 與 revision。驗證及原子 mutation 的不可分割性是假設；只在 snapshot 讀取時驗證不是它。
- **E-contract / epoch_conflict**：snapshot authority row 與 renewal 共用衝突範圍，於 write admission 及 commit 保證 epoch 仍一致。兩個檢查點是同一 read-set validity 契約的實現；commit 檢查覆蓋 mutation 後才發生 renewal 的情形。
- **R-contract**：certifier 只用 acquired 新 epoch 的當前 committed 狀態，不沿用舊 snapshot/cache。certification 不預設「任何未來舊寫都失敗」。
- **K/Q/frame**：K 不重用；唯一接受入口是 Commit；有效 P/v/E/Q 原子更新；非 Commit 不能改有效狀態；Q 不被 ACK loss 刪除。本輪不弱化這些條件。

三個且只有三個 configurations：baseline=(T=true,E=true)；A=(T=false,E=true)；B=(T=true,E=false)。A 不移除初次 pending/revision check、atomic installation、K no-ABA 或 E。B 不移除初次 request epoch check、T、renewal 或 fresh-read guard。沒有第三項弱化，也沒有 whole-database SQLite snapshot check 被事後移除：它從一開始就不屬於這個拆分契約模型。

## 4. 與轉移分離的安全性

`properties.py` 的 trace monitor 記錄每個 Commit(i)，不參與 transitions() 的 guard。

- **P2(a)**：每個有限 reachable prefix h，`|Accept_K(h)|≤1`。
- **P2(b)**：若 h 中已出現新 epoch certificate C，後續不許發生 `request_epoch(i)<C.epoch` 且改變有效 `(P,v,E,Q)` 的 Commit(i)。這是「沒有認證後新生的舊 authority 有效轉移」的嚴格版本；**不要求整個 durable state 永遠等於 C**，新 epoch 合法提交可改變它；舊 commit 的遲到 response 也不違反它。

對本固定 payload 模型，每次 Commit 都新增 E/Q，所以每個認證後的舊 Commit 都屬實質不一致，而非無效果的 no-op。P2(b) 不由 late manager error 或 physical append 觸發。

額外結構 audit（assert，不是 scheduler guard）：`v=3+|Accept|`、`P⇔|Accept|=0`、E=Q=Accept、接受者已 append、成功 manager result 必須有 commit、certificate 等於認證當刻的狀態、非 Commit 不改有效狀態。這些 audit **允許 count=2 / v=5**。若它們失敗，屬模型/explorer harness 失敗，不能當作安全反例或 Temporal RED。

## 5. 探索與非空泛性

Python standard-library unweighted BFS，完整保存 visited/parent/depth；無 depth/state cap，無 partial-order reduction，沒有遇見第一個反例便停止。對第一個違反的 state 回溯 parent，得到以本表單位動作計量的最短反例；不是所有等長反例的列舉。進程有資源 timeout；若未耗盡 queue，必須標 INCONCLUSIVE，不能報 exhaustive PASS。

除結果外，記錄可達 count 最大值、certificate 類型、cert 後舊交易仍 open、commit 已發生但 caller unknown、新 epoch cert 後合法 commit、unknown 後 retry、終止狀態等診斷。A 若不能到 count2、B 若無法表達 post-cert old commit，需查模型限制，不能單憑 baseline PASS。

Main/Positive/Negative 是從**同一個已探索 baseline graph**抽取的見證路徑。另抽取 ACK-loss/retry 路徑檢查 observation 分離；不是第四種配置或第三項契約弱化，也不是新的 Temporal 實驗。

## 6. 明確不成立的外推

此模型沒有證明 SQLite、PostgreSQL、Cassandra 的可達排程相同；SQLite 整體 snapshot conflict 可能同時保護 T/E，真實資料庫的 row constraints 也可能額外保護。模型錯誤字串不是 driver code517。初始 read 在一個動作內讀 epoch/MS，fresh certificate 在一個動作內讀完整有效 tuple；真實多次查詢與六類 task enumeration 的一致性要另有穩定窗或 snapshot 證明。

沒有 crash、task delivery/retirement、owned-epoch transfer、原生 persistence timeout、無限重試或公平性模型，因此不證一般 progress、外部 exactly-once、full Temporal refinement 或完整 Worker indistinguishability。manager/caller 只建模與本安全性相關的觀察摘要，不冒稱完整 Worker observation function。
