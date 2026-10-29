# CCPO / EGTS D-2：有限狀態驗證與契約獨立性

> 公開副本：保留原始研究結論與結果，只整理本機路徑、來源連結及保全metadata；詳見 [出版說明](PUBLICATION-NOTES.zh-TW.md) 與 [hash對照](ORIGINAL-TO-PUBLIC.json)。本次未重跑模型。

2026-10-26。範圍只有 D-1 的兩個 P2 子命題；沒有新的 Temporal 實驗、production 修改、編譯、GitHub 寫入、commit 或 push，沒有開始 D-3。

**結論：在明列的有限抽象模型內，P2(a) 與 P2(b) 的候選契約足夠，而且兩項保護可分離。** 基準的 23,385 個 reachable states 全部通過。只移除 commit-time claim 驗證，得到 12 步重複接受反例，但 fencing 性質仍成立；只移除 epoch read-set 保護，得到 10 步認證後舊寫反例，但至多一次接受仍成立。這不是 Temporal RED，也不是 SQLite／所有 backend 的一般正確性證明。

## 1. 證據與版本

本文標籤：**Model-defined** 是本輪明訂的抽象；**Model-verified** 是本次 Python 全列舉結果；**Model-inferred** 是數學推導或解釋；**Runtime-supported（既有）** 僅引用已保存 Temporal runs；**Source-confirmed（承接）** 僅表示讀到既有來源／版本；**Unknown** 是尚未證明的映射或外推。Model-verified 不等於 Temporal runtime-verified。

| 來源 | 精確版本／用途 |
|---|---|
| D-1 | [D1_ORIGINAL_REPORT 第7／9節摘錄](provenance/D1-P2-D2-excerpt.zh-TW.md)；SHA-256 `f425383ff2a5e832378c3a78b1172a049e3fa34951e3fe125c2779cfde89eac4`，第 7 節 P2 與第 9 節 D-2 建議 |
| C-5B checkpoint | `c86de99134b3a1d4144e089d6c1bba35e29debcc`，personal SDK-Rust research repo；只讀既有 public report、comparison 與 manifest 所列證據 |
| 原 Server baseline | `0d5e3848bed625907987f73d5eaae8dc77bc6590`；本輪未重新分析／執行 Server |
| Python | 3.12.3，僅 standard library；唯一一次探索為 `run-01` |
| 公開包位置 | `research-evidence/d2-finite-state/`；原始封存與此公開副本分開保存 |

探索前已保存數學規格、預測及三個 source；本包提供 [MODEL.zh-TW.md](MODEL.zh-TW.md)、[預測公開副本](PREREGISTRATION.public.json) 與 [原始探索前雜湊](provenance/original-PRE-RUN-SHA256SUMS)。原D-1/C-5B的67個輸入核對與本機Git指紋留在私有封存；公開索引見 [PROVENANCE.json](PROVENANCE.json)。它們不是本次新增的產品執行證據。

## 2. 最小模型與局部契約

〔Model-defined〕固定一個 normal、已 Started 的 WFT **K** 與相同 payload **p**；兩個 attempt 0/1。retry 保持 K/p，但服務端可依新的 authority 建立新的 internal persistence request；不是要求 Worker 自行選 RangeID。attempt 身分、WFT 身分、shard epoch、Mutable State revision 四者分開。

`s=(r,u,P,v,H,E,Q,T0,T1,C)`：r 是 durable epoch1/2；u 是 authority lifecycle；P 是唯一 pending K；v 初始3；H 是 physical append；E 是有效 batch ledger；Q 是 durable task 建立 ledger；Ti 含 immutable request epoch/revision、首次實讀 snapshot、transaction stage、manager/caller/ack；C 是新 epoch fresh certificate。完整初始值與每個轉移的 guard/effect 見 [模型規格](MODEL.zh-TW.md)。

關鍵階段順序為 `Submit → AppendPhysical → Begin → ReadSnapshot → TryMutation → Commit/Abort`。Begin 不等於第一次 read；TryMutation 僅形成 private write-set，只有成功 Commit 能改有效 P/v/E/Q。Range renewal、Acquire、CertifyFresh 可插入交易階段之間。ManagerUnknown 不終止 delegate；正常 manager result、ACK sent、ACK received/lost、caller timeout 分開。

| 契約 owner／邊界 | 可獨立檢查的操作條件 | 對應 D-1 |
|---|---|---|
| WFT identity／生命周期 | K 唯一、不重用；沒有把 consumed K 改回 pending 的其他入口 | P2-K |
| Acceptance transaction | live pending/revision 驗證與消耗、有效 history/MS/Q 安裝在同一原子 commit；失敗不安裝有效 batch | P2-T |
| Persistence／authority row | request epoch 的初次驗證；snapshot authority read-set 在 mutation/commit 維持有效，與 renewal 共用 conflict domain | P2-E |
| Recovery／reader | renewal 已提交且 acquired，才取當前 committed tuple；不使用舊 cache/snapshot | P2-R |
| Durable work／transport | Q 與接受原子關聯；ACK loss 不刪 obligation；ACK sent/received 不改 durable acceptance | P2-Q（本模型沒有 task retirement） |

這些條件不是「每個可能世界都安全」的定義；它們是 transaction guard、write-set、原子性、fresh-read 及 frame 的具體限制。但它們仍是**抽象介面假設**，不是已機械證明 Temporal 所有 writer 都遵守的事實。

〔Model-defined〕v 每個有效接受加1，沒有硬編碼 `v≤4`；E/Q 是可追加兩筆的序列，沒有以 K 為 key 的隱藏去重 set；AcceptCount 是外部 monitor，不作准入 guard。兩個 attempt 自然限制最多兩次 Commit，而非把欲驗證的上限1寫入狀態。A 實際達 E/Q=[0,1]、count2、v5。

## 3. 與規則分離的性質及執行方法

〔Model-defined〕`properties.py` 定義：

- **P2(a)**：每個有限 reachable prefix h，`|Accept_K(h)|≤1`。
- **P2(b)**：已有新 epoch certificate C 後，不得再發生 `request_epoch<C.epoch` 且新增有效 P/v/E/Q 的 Commit。這不禁止新 epoch 合法寫入，也不把舊 commit 的晚回覆視為晚 commit。對本模型每筆 Commit 都新增 E/Q，所以不是可忽略的 no-op。

P2(b) 是「認證後沒有新生的舊 authority 有效轉移」，**不是所有未來狀態都等於讀到的快照**。讀前已 committed 的舊交易必須由 fresh certificate 反映；讀後新 epoch 的合法 continuation 可改變該狀態。

〔Model-verified〕[explore.py](source/explore.py) 用 FIFO BFS、完整 State+Monitor 去重、首次訪問 parent/depth；沒有 state/depth cap、partial-order reduction 或違反後停止。三個 queue 均耗盡。保存第一個 bad state 的最短 parent trace，並確認它的每個 proper prefix 均未違反該性質。外層進程 timeout600秒未觸發。

唯一歷史執行（[invocation公開副本](run-01/invocation.public.json) 僅將 argv/cwd 的本機根目錄正規化；以下不是新執行）：

```text
python3 -B source/explore.py --output run-01/results
```

退出碼 **0**；stderr **0 bytes**；外層 wall time **1.227859886999795 秒**。退出碼0本身不代表兩個弱模型安全；每項性質另在結果中列明。所有預測於執行前保存，沒有因結果改動 source。

| 檔案 | 執行 source SHA-256 |
|---|---|
| `source/model.py` | `4a3d4a39ba571013e2fb1c2a838e75e75d3477d861baa3276b1d28361b74a417` |
| `source/properties.py` | `c31efbc29caff7da8ad86a141926a62eb35f3eee9d1d573ac7fb7dfbdd24cdac` |
| `source/explore.py` | `918348c37d8a465fd0fa2f3aec1192d8e96111452f6a67bdae432bfe80d7372f` |

## 4. 完整可達性結果

〔Model-verified；[summary.json](run-01/results/summary.json)、[完整 stdout](run-01/stdout.log)〕

| 配置 | reachable states（含 monitor） | edges | 最大最短深度 | P2(a) | P2(b) | 最大接受數／revision |
|---|---:|---:|---:|---|---|---|
| baseline：T/E 皆保留 | 23,385 | 89,646 | 26 | PASS | PASS | 1／4 |
| A：只移除 commit-time claim | 23,105 | 88,274 | 26 | **反例，最短12步** | PASS | **2／5** |
| B：只移除 epoch read-set conflict | 17,371 | 64,974 | 25 | PASS | **反例，最短10步** | 1／4 |

共檢查 63,861 個配置內狀態、242,894 條邊。A 有1,800個帶 P2(a) 違反的狀態；B 有1,590個帶 P2(b) 違反的狀態，這不是1,800／1,590種獨立根因。弱化後狀態總數較少不矛盾：原來 guard-failure 的錯誤分支被成功分支取代，並非僅往原圖加入邊。

非空泛診斷：baseline 有 **948** 個「certificate 後舊 snapshot transaction 仍 read/mutated」狀態、**255** 條 certificate 後新 epoch 合法 Commit 邊、**5,945** 個已 commit 但 caller unknown 狀態。certificate 包含 pending 與 consumed 兩類。A count2 與 B post-certificate old commit 均實際可達，未由模型上限排除。

## 5. 兩條最短反例及第一分歧

〔Model-verified〕完整逐步狀態在 [A counterexample](run-01/results/A_no_commit_claim/counterexample-P2a.json) 與 [B counterexample](run-01/results/B_no_epoch_conflict/counterexample-P2b.json)，可讀表另見 [COUNTEREXAMPLES.zh-TW.md](COUNTEREXAMPLES.zh-TW.md)。

**A，12步。** `0:Submit→Append→Begin→Read(r1,v3,P)→TryMutation`；`1:Submit→Append→Begin→Read(r1,v3,P)`；`0:Commit`；`1:TryMutation→Commit`。第一次提交令 P=false/v4/E=[0]；第12步第二次提交令 v5/E=Q=[0,1]。兩個初次 read 都合法，epoch 始終1，只有 commit-time pending/revision protection 被拿掉。**第一分歧是第12步**：baseline 會產生 `CommitRejected(claim_conflict_at_commit)`，A 卻 Commit。沒有弱化 no-ABA、初次驗證或 Q 原子安裝。

**B，10步。** `0:Submit→Append→Begin→Read(r1,v3,P)→TryMutation`；`RenewBegin→RenewCommit(r2)→Acquire→CertifyFresh(v3,P,E=Q=[])`；`0:Commit`。第10步舊 request r1 新增 v4/E=Q=[0]；此時 live claim 仍合法，故 T 不能阻止舊 authority。**第一分歧是第10步**：baseline 會 `CommitRejected(epoch_conflict_at_commit)`，B 卻 Commit。初次 request-vs-range check 完整保留；若舊 request 在 renew 後才第一次 read，它仍會被拒絕。

B 的最短 trace 在 renewal **之前**已形成 private write-set；並不是 Main 的「read snapshot 後、mutation 前跨屏障」物理排程。這正好檢查 mutation 後才更新 epoch 的窗口。模型中的 TryMutation 不取得實際 SQLite writer lock；不能據此斷言這條完整物理順序在 SQLite 可達。

〔Model-inferred〕最短性另有直接下界：A 的兩次接受各必須經 Submit/Append/Begin/Read/TryMutation/Commit 六步，共至少12；B 的一次接受至少6步，加 renewal/acquire/certificate 四步，至少10。保存的 trace 分別達到下界。最短反例不含 ACK loss，表示這兩種缺失本身不需 timeout 才會出錯；本輪沒有為了符合 ambiguous-commit 敘事強加非必要事件。

## 6. 歸納論證與條件獨立性

**P2(a) 的歸納論證。**〔Model-inferred〕不變式為 `AcceptCount=0⇔P`、`AcceptCount≤1`，加唯一 Commit 入口的 frame。初始 count0/P=true。非 Commit 不改 P/AcceptCount。當 count0 時，合法 Commit 在同一步驗證 live P=true 並令 P=false/count1；當 count1 時 P=false，下一個 Commit guard 必失敗。沒有規則重建 P，排除相同 K 的 ABA。故所有 reachable prefix 至多一次。E、fresh read、ACK結果均不參與此證明。Q/E 與該接受原子建立且其他動作不新增 Q/E，才可將 count 結論推及本 command batch 的邏輯 durable work 建立次數。

**P2(b) 的歸納論證。**〔Model-inferred〕對任何成功 Commit，初次合法 read 已建立 `request_epoch=snapshot_epoch`；commit-time E 驗證又建立 `snapshot_epoch=current_epoch`。有效 C 在新 epoch2 committed/acquired 後產生，epoch 永不回退。因此 C 之後 request_epoch1 不可能同時等於 current_epoch≥2。非 Commit 沒有有效 mutation，故不會繞過此論證。讀前的 Commit 在 certificate 中可見，來自 fresh-current-tuple 規則；遲到 result/ACK 不動 P/v/E/Q。T 的 pending/revision 檢查不參與排除舊 authority 的證明。

這是明列轉移規則的紙上歸納證明，尚未輸入 proof assistant。有限 BFS 與該論證相互檢查，但不是對任何未建模 backend 的證明。若把相同規則延伸至任意 attempt 數及單調 epochs，兩個歸納步不依賴「只有2」；**一般化仍以唯一入口、不可重用、原子性、單調性與 frame 完全成立為條件**，不是從有限通過歸納到 production。

| 條件 | 本輪可支持的結論 | 不能聲稱 |
|---|---|---|
| T bundle：commit-time pending＋revision | 相對其餘固定規則，去掉整組就可破 P2(a)；E 不能補上 | pending 與 revision 各自皆不可缺 |
| live pending 原子消耗 | 在 K no-ABA 下，單獨足以作 P2(a) 歸納 guard | 不考慮 K reuse、其他接受入口仍永遠足夠 |
| revision CAS | 在初次 pending 驗證、revision 每次成功增加且不重用下，也能排除第二次舊 snapshot 提交；因此此小模型的兩個 claim 子 guard 互有冗餘 | 所有 Temporal revision 都只有 WFT acceptance 會改；或 CAS 自動保證 Q 原子性 |
| E bundle：mutation/commit epoch read-set protection | B 表明相對其餘規則可獨立缺失並破 P2(b)；T 不能補上 | 真實 SQLite 有兩個可各自關掉的物理保護機制 |
| mutation-time epoch check | 相對完整 commit-time epoch validation，對 P2(b) 安全性屬提前拒絕的冗餘 guard | 只在 mutation 前檢查就足夠；B 的現有反例在 mutation 時 epoch 仍一致 |
| 初次 range check | 綁定 request 與 snapshot，亦表示 Negative 的拒絕路徑 | 一次早期 range check 能保護稍後 commit |
| R 的 fresh tuple | 建立「證書反映當刻已提交效果」的讀取語義；本輪直接保留並 audit | P2(b) monitor 已證明完整 tuple freshness 必要；該 monitor 對禁止後續舊寫只用 genuine certificate epoch |
| K/Q/frame／ACK retention | 本輪共同保留，歸納及 work-ledger 解釋需要它們 | 本輪以第三項弱化證明其必要性，或 task retirement/crash 已被測到 |

上表的子 guard 冗餘是書面推導，**未另跑 pending-only、revision-only 或 mutation-only 等配置**。本輪只有預先登錄的兩項弱化。

R 的細分是對 D-1 的重要精確化：應區分 **authority certificate authenticity**、**absence of post-barrier old commits**、**fresh evidence completeness** 三個義務。本輪 P2(b) 主要檢查第二項，第一項及完整 fresh tuple 規則固定為前提；不能把它的 PASS 當作所有 read freshness 必要性與 sufficiency 都經過 ablation。

## 7. 對既有三組 Temporal witness 的映射

〔Runtime-supported（既有）＋Model-verified（抽象路徑）〕三個路徑均逐步確認存在於同一個已耗盡的 baseline graph，未再跑 Temporal。這是局部 trace inclusion，沒有證明實作與模型的雙向 simulation。

| 既有 witness | 抽象模型路徑／終態 | 映射界線 |
|---|---|---|
| Main | read r1/v3 → manager unknown → renew2/acquire/cert v3/P → 舊 TryMutation 失敗/Abort → 新 attempt r2 接受一次/v4；20步，E=[1] | 模型的 epoch conflict 對應已記錄的實際 SQLite517；模型不產生或重新觀測 driver517 |
| Positive | 舊 Commit r1/v4 → manager unknown → renew2/cert v4/consumed → 同 K retry 入口拒絕；15步，E=[0] | 保留 commit 早於 caller outcome／renewal，不以 error推斷 commit；入口拒絕是抽象，不逐層模擬 NotFound RPC |
| Negative | 先捕捉 request r1 → manager unknown → renew2/cert v3/P → delegate才 Append/Begin/read實際r2 → range mismatch/Abort → r2合法retry/v4；19步，E=[1] | 區分 request r1 與第一次DB實讀r2；對應既有 ShardOwnershipLostError，無新 SQL驗證 |

完整 paths 在 `run-01/results/baseline/witness-{Main,Positive,Negative}.json`。額外 `witness-AckLostRetry.json` 取自**同一 baseline graph**：Commit → ManagerResult(success) → SendAck → LoseAck → ObserveTimeout → 同 K retry被拒絕；14步，仍只有一次接受。它不是第四種配置、第三項弱化或 Temporal 第四個實驗。

〔Model-inferred〕caller unknown 不要求 transaction unknown to the omniscient explorer；三者須分開：實際 durable outcome、manager得到的結果、caller收到的觀察。服務端安全的再提交可以無需 caller 判明最初是哪個世界。這並未定義／驗證完整 Worker observation function，也沒有重做 C-1 indistinguishability proof。

〔Unknown／明確抽象差異〕Q 只表示本批 durable task 的邏輯建立 ledger，不重做六類實體 task enumeration；H/E 不是實體 protobuf/version-history branch。真實 C-5B polling 後 v5 是 ActivityStarted 等後續更新，本模型沒有 polling；A 的 v5 是第二次接受，兩者不能混用。正常原 SQL 若有 event-id／row uniqueness、整體 snapshot conflict 等額外限制，可能阻止 A/B 的實體化。這些從一開始就不在拆分契約模型，不是為製造反例事後刪掉的規則。

## 8. 研究價值：五個直接回答

1. **P2 契約是否足夠？**〔Model-verified＋Model-inferred〕對本有限模型足夠；兩項均 exhaustive PASS，並有依轉移歸納的理由。這個回答以 K不重用、唯一接受入口、原子有效狀態更新、epoch monotonicity、fresh read/frame 等固定前提為限。
2. **何者獨立必要、何者冗餘？**〔Model-inferred〕在本模型的其餘規則不變時，T bundle 對(a)、E bundle對(b)分別不可整組刪掉；它們互相不蘊含。pending/revision 子 guard、mutation/commit epoch 檢查有上述條件式冗餘。不能稱所有具體 guard 或所有 backend 實現方式普遍必要。
3. **模型揭露了什麼？**〔Model-verified＋Model-inferred〕至多一次接受完全可能與 authority barrier violation 同時成立；有完整 epoch fencing 也可接受同 K 兩次。反例都不需要 ACK loss。mutation時驗證過的 authority 會在commit前失效；fresh tuple 的可信性又是不同義務。這些區分避免把 D-1 的條件當成不可分的一大包。
4. **是否新理論？**〔Model-inferred〕核心是標準的 atomic claim/CAS、read-set validation、fencing 及 assume-guarantee reasoning，**不主張新的分散式不可能性或新 fencing 定理**。有意義的組合成果是把 identity/claim 與 authority/read 接口拆開、明列共享 frame、以獨立可違反性和 existing-witness embeddings 檢查跨模組接線；研究價值主要在規格精確化和可否證性。
5. **繼續、修正或停止？**〔Model-inferred〕建議先修正 CCPO 的條件粒度，保留這兩個受限 lemma，不立即宣稱一般定理。若未來繼續，最窄缺口是明訂 backend-to-model simulation：哪些真實 writer/renewal/read 共同實現 atomic claim、epoch有效性與 fresh evidence，強 snapshot primitive 是否一次涵蓋兩者。若無法提出比標準 transaction契約更精確、可獨立檢查的映射，此方向應停在整理與驗證成果，不為新穎性擴張主張。**本輪不實作或開始 D-3。**

## 9. 限制、否證邊界與保全

〔Unknown〕未建模：真實SQL writer locks、完整 isolation hierarchy、crash/power loss、context/goroutine ownership、owner/process transfer、native persistence timeout、任意多次 renewal、K reuse、task retirement/移交、語言結算、外部 effects及不公平無限執行。兩attempt／兩epoch exhaustive 不等於 unbounded model checking，更不等於一般 Temporal refinement。詳細列於 [LIMITATIONS.zh-TW.md](LIMITATIONS.zh-TW.md)。

基準若在全部操作前提仍成立時出現第二次接受或認證後舊有效commit，才否證對應抽象命題。若真實 backend 不提供它們，屬 mapping/assumption缺口。若 trace無法重播或結構assert失敗，屬 explorer/model harness問題；不能稱 Temporal CORRECTNESS RED。A/B 是刻意弱模型反例，符合執行前預測。

〔Model-verified／原始保全摘要〕模型source、規格、預測與原run-01的雜湊對應維持不變。原始嚴格audit因新增工具內部capture ref而exit1；後續audit明列差異後exit0，不宣稱所有Git refs完全不變。原始D-1／C-5B輸入及研究refs未變。公開包不包含本機Git指紋與內部ref識別；保留原始hash和限定摘要於 [PUBLICATION-NOTES.zh-TW.md](PUBLICATION-NOTES.zh-TW.md)。本包以 [SHA256SUMS](SHA256SUMS) 驗證公開檔案；本次準備未重跑探索或修改原始證據。

本輪到 D-2 停止。
