# D-1 原始報告第 7／9 節摘錄

以下兩節從 D1_ORIGINAL_REPORT 逐位元組擷取，沒有改寫數學內容；這是範圍摘錄，不是完整 D-1 報告。原始全文 SHA-256：`f425383ff2a5e832378c3a78b1172a049e3fa34951e3fe125c2779cfde89eac4`。精確行號及每段擷取 hash 見 [PROVENANCE.json](../PROVENANCE.json)。

原文 R7 指既有 [C-5B checkpoint](https://github.com/hikaru-212/temporalio-sdk-rust/tree/c86de99134b3a1d4144e089d6c1bba35e29debcc/research-evidence/c5b-range-fencing)；R4/R5/R6 分別是既有 C-2/C-3/C-4。D-2 的完整自足模型規格另見 [MODEL.zh-TW.md](../MODEL.zh-TW.md)。摘錄中的「本輪未開始 D-2」是 D-1 當時的時序陳述，不是本公開包的新結果。

**7. 命題二：單次消耗式持久接受與新 authority 屏障的可分解組合。**

〔Model-inferred：受限安全命題候選 P2〕範圍為 normal、已 Started 的固定 WFT K，排除 Reset／K reuse、speculative／paginated completion、跨 cluster。D 的實際值不由 Worker timeout 決定。

可檢查操作規則：

1. **P2-K 身分。** completion 與重送保持 `(K,p)`；後繼 WFT 不重用 K，其他轉移不得重新建立已消耗 K 的資格。相同 K 不同 p 的語義另定，本命題只比較相同 p。
2. **P2-T 原子接受。** 每次有效 completion 只能由一個 transaction 原子執行：驗證當前 `pendingK=K` 與預期 revision v；消耗 pendingK、遞增 v、更新有效 history frontier／ActivityInfo，並建立本批 command-generated Q。條件失敗不能殘留這些有效轉移。物理 history append 可在外，但不因存在就變有效。所有接受途徑都經此 primitive。
3. **P2-E epoch 衝突。** 每次有效 mutation 的 range validation 讀取、renewal 寫入同一 authority row；這些讀寫與有效 mutation commit 共同受 lock、commit-time read-set validation 或具體 backend conflict 原語約束。若快照早於 renewal，仍可能寫入者必須被序列化到 renewal 之前，或因 read/write conflict 中止；不能只在獨立、早已結束的讀取中比對 r。新 range 單調且不重用。
4. **P2-R fresh read。** renewal 已 commit；當前 acquired guard 才開放讀取。fresh read 具有已提交版本可見性，不能沿用舊 transaction snapshot／失敗操作 cache；若 authority 再變或需多個不一致來源，重新認證。有效 history 由同一 persisted MS 的 branch/frontier 解釋。獨立多表 task enumeration 僅在穩定觀察窗可作 oracle，不假定六類掃描是單一 atomic snapshot。
5. **P2-Q obligation。** 已建立 Q 或它合法移交的 obligation 不因 ack loss／cache eviction 被丟棄；刪除需符合既定交付／接手規則。這不要求所有歷史 task rows 永久存在，也不保證 Activity 外部效果。

P2 包含兩個獨立子結論，不把它們偽裝成互相的必要前提：

- **P2(a)，僅用 P2-K/T：** `∀h reachable, AcceptCount_h(K)≤1`；該 command batch 的 durable Q 只在那次原子接受建立。這是邏輯建立次數，不等於重送／搬移後所有 physical rows 總數永遠為一。
- **P2(b)，用 P2-E/R：** 若 r′>r 的 renewal 與 certified fresh read 已完成，仍攜 r 的舊 persistence attempt 不能之後新生該 read 未反映的有效 D/Q transition。它可已在 barrier 前提交（read 必須反映其有效後果），或之後 abort/reject。此結論不禁止新 epoch 的合法寫入，也不禁止舊 attempt 遲到的「結果回傳」。
- **組合後的續行安全（非第三命題）：** P2-K/T/E/R/Q 使同一 `SubmitSameCompletion(K,p)` 可在兩種 outcome 安全處理：已消耗 K 則拒絕再次接受；仍有效 pending 則由當前 authority 接受一次。初始 caller 無須先知道第一次是否 committed。若 K 已合法失效，走拒絕／successor recovery；不能保證舊 K 成功。

證明草圖：對 commit-order 歸納，首個成功 transaction 消耗唯一 K，第二個同 K claim 因 pending/revision 條件失效；不可重用排除 ABA。Q 與 claim 的原子關聯排除「claim 一次、Q 兩次／零次」窗口。再把 old transaction 相對 renewal 分成先序列化、後驗證或 snapshot 衝突三類，結合 fresh visibility 排除 barrier 之後才生效的舊 authority mutation。最後由 Q 保存規則證明 ack 丟失不抹除已接受的工作義務。

非循環性：前提是可檢查的唯一資格、transaction read/write-set、condition、commit/abort 原語、read admission 與 retention，而非直接假設 `AcceptCount≤1` 或「所有世界安全」。P2-E 是 backend proof obligation；「同一 transaction 中有 range read」本身不足。此抽象尚需證明所有 production writer、recovery 路徑與 backend 都符合，不能因三次 PASS 直接視為成立。

反例與否證：X5 擊中 epoch/read-set 契約，X6 擊中 claim/no-ABA，X7 擊中 Q 原子性。真正反駁 P2 需要在前提均成立的可達執行，找到同 K 二次有效接受，或 certified new-range read 之後 old-r attempt 的新有效 mutation。若只是更強前提在實作不成立，分類為映射／相容性缺口。NotFound、Timeout、orphan history、單一錯誤碼或 recorder 未發現 row 都不充分。

〔Runtime-supported，R7〕既有三組正好是三個有限 witness：

| Main | Positive | Negative |
|---|---|---|
| r1 真實讀 snapshot 保持開啟→controlled error→renew r2→guarded read v3/pending→release→mutation 得517→rollback→獨立 v3 不變→合法 retry v4 | r1 transaction commit→controlled error→renew r2→fresh v4/accepted→同 K retry 拒絕且 ledger 不變 | controlled error 先於 delegate→renew r2→fresh v3/pending→攜 r1 的 delegate 才開始→真實讀 r2→ownership error／未 mutation→合法 retry v4 |

三組最終各有一個有效 Completed4／ActivityScheduled5，相關六類 task 直接分頁至 EOF，Activity polling 成功但不執行外部 Activity。Manager-level early TimeoutError 是受控 outcome simulation，不是 SQLite 自發 commit-timeout。不可把 privileged harness 對 committed/pending 的辨識能力當成 Worker 本來就有的觀察。

〔Source-confirmed（承接）+ Unknown〕R4 支持 token/pending/revision/range 接線與 ack 分離；R5 支持有限 response-loss 與 concurrent 情境但無完整 task-range absence oracle；R6 增加六類 enumeration、真實 recovery，尚無 old SQL跨屏障。R7 補上該特定 SQLite snapshot schedule，但 native timeout、power-loss、crash、owner/process transfer、其他 backend、所有合法排程與完整 Worker observation-equivalence 仍未證。immutable request deep copy、bounded delegate context、done synchronization、connection teardown 是 harness→模型可信映射條件，不是理想原子模型的普遍數學必要條件。

條件式進度另需最終服務／authority 穩定、公平重試與 task 處理、仍有效 K 或可辨識 successor、未丟 obligation。有限 polling 成功只證一條 recovery path，不能推出無條件 eventual completion。

**9. 唯一優先的 D-2 建議；本輪不開始。**

〔Model-inferred〕建議先研究 P2 的純有限狀態模型，而非同時合併語言語意、persistence 與外部效果。範圍：一個固定 normal K、兩個 completion attempts、epochs1/2、revision 類別3/4/≥5、pending/consumed、AcceptCount 類別0/1/≥2；必須容許表示第二次接受，不能由狀態上界排除欲找的反例。分開 physical history、effective frontier、Q 建立與 ack sent/received。舊交易至少區分 Begin、建立 read snapshot、range check、mutation、commit/abort、manager return；renewal/read 另為動作，保留 manager return 與 physical outcome 的不同次序。

預先目標為 P2(a) 與 P2(b)，同一小模型內作兩個單變量規則弱化：第一項只將 commit-time pending/revision claim 檢查改為早先 snapshot 的一次檢查，保留 K 不重用，預測可 duplicate admission；第二項只移除 epoch read-set conflict，預測可有 abstract post-barrier stale mutation。no-ABA 的 X6 仍是另列的抽象限制，不在 D-2 增加第三個 ablation。第二項是弱模型反例，不能冒稱 SQLite可達。保留契約時任意丟 ack 應不破壞 safety；進度只另加公平性後檢查。模型應能投影到 R7 三個既有 witness 的事件偏序，但不靠把所有其他排程刪掉來「證明」結果。

輸出應是可列舉的 reachable-state／inductive invariant 證明與最小反例，不需要新 Temporal source、服務或 DB。仍須先決定版本與 authority primitive 的精確抽象規則、Q retirement／移交的安全邊界；不把 model checker 通過當成 Temporal 全實作 refinement 證明。本輪未寫模型、未執行 model checking、未開始 D-2。

