**C-1 可以建立一個比 #1597 更強的結果：即使 W、S 與通道都遵守已宣告的契約，W 仍可能無法判定第一次操作是否提交。若協定既不提供辨識結果的能力，也不讓重複請求安全，就無法同時保證「不重複效果」與「最終完成」。**

這是下述受限模型的結果，不是 Temporal bug 的判定。等待、查詢、reconciliation，以及使用去重協定重試，都必須分別分析。

本輪只有唯讀資料研究，沒有修改檔案、編譯、測試或操作 GitHub。以下 **Model-inferred** 表示依明列規則推導；**Source-confirmed** 表示一手文件支持；本輪沒有新增 runtime 證據。

**1. 最小轉移系統。**

[Model-inferred] 固定一個邏輯操作 \(k\)，其內容為 \(p\)。例如，它要求在外部資源新增一次不可重複的業務效果。

必須區分：

| 符號 | 意義 |
|---|---|
| \(k\) | 邏輯 operation identity，重試時不變 |
| \(a\) | attempt identity，每次呼叫不同 |
| \(e\) | ownership epoch |
| \(D[a]\) | S 的實際 durable commit record |
| \(F\) | 已發生的外部效果紀錄；\(n_k\) 為 k 的效果次數 |
| \(A_s,A_r\) | acknowledgement 已送出、已被 W 收到的紀錄 |
| \(J_W\) | W 保存的操作意圖、嘗試與觀察紀錄 |
| \(B_W\) | 與 W 觀察相容的可能世界集合 |
| \(Auth\) | 實際權限；不同於 W 自己相信仍有權限 |

F 是模型用來描述實際效果的環境狀態，**不代表 W 已有查詢 F 的 API**。

基準協定只有 `Execute(k,p,a,e)` 與成功回覆，沒有 deduplication、status query、自動 acknowledgement 重送或取消屏障。通道可以遺失、延遲、重排訊息，但不偽造訊息；每個訊息至多交付一次。

| 元件 | 狀態與轉移 |
|---|---|
| **W** | 保存 `Intent(k,p)`；取得權限後送出 attempt；收到相符回覆則記錄成功；期限到但沒有回覆則進入 `Unknown` |
| **S** | 收到有效請求後依序執行 `Commit → Effect → SendAck`；每個收到的 invocation 獨立處理，不依 k 去重 |
| **通道** | `Enqueue → Deliver`，或 `Enqueue → Drop`；逾時不撤銷已送出的請求 |
| **Crash／Recover** | volatile 執行位置可以消失；D 與已發生的 F 保留；恢復不憑空產生成功回覆，也不自動知道缺少紀錄的外部結果 |

其中：

\[
Commit(a):D[a]\gets Committed(k,p)
\]

\[
Effect(a):F\gets F\mathbin{\|}(k,a),\quad n_k\gets n_k+1
\]

\[
SendAck(a):A_s\gets A_s\cup\{a\}
\]

\[
ReceiveAck(a):A_r\gets A_r\cup\{a\}.
\]

**Commit 與 Effect 是兩個轉移。** 此模型的 commit 表示 S 已持久接納該操作及其履行義務，不表示外部效果已完成。停在兩者之間的狀態合法，因此：

\[
D[a]=Committed\centernot\Rightarrow n_k=1.
\]

基準案例固定權限有效且不變，避免把 ownership ambiguity 混入 commit ambiguity。兩個主要世界也都不需要 crash。

S 的局部契約是：不偽造成功、commit 持久保存、每次收到的 invocation 按規則處理。它**沒有承諾跨 invocation 的 at-most-once 效果**。通道遺失訊息也符合其契約。

**2. 兩個明確可達的世界。**

[Model-inferred] 初始狀態為：

```text
D=∅，F=[]，As=∅，Ar=∅
W 已保存 Intent(k,p)，持有有效權限 e
```

兩條合法路徑：

| 世界 A：提交成功、回覆遺失 | 世界 B：請求遺失、未提交 |
|---|---|
| W 送出 `Execute(k,p,a0,e)` | W 送出相同請求 |
| 通道交付請求給 S | 通道丟失請求 |
| S 執行 Commit(a0) | S 沒有收到請求 |
| S 執行 Effect(a0) | 沒有 commit 或 effect |
| S 執行 SendAck(a0) | 沒有 acknowledgement |
| 通道丟失 acknowledgement | — |
| W 在 T 時逾時 | W 在相同 T 時逾時 |

終點的真實狀態不同：

| 事實 | A | B |
|---|---:|---:|
| \(D[a_0]=Committed\) | 是 | 否 |
| \(n_k\) | 1 | 0 |
| acknowledgement 已送出 | 是 | 否 |
| acknowledgement 已收到 | 否 | 否 |
| W 觀察到逾時 | 是 | 是 |

每個轉移都在前述規則內；不需要錯誤回覆、資料損毀或違約元件。

此外，belief state 還可能包含「已 commit、尚未 effect」等中間世界，不能只保留 A、B 兩個選項。

**3. 完整觀察函數與不可區分性證明。**

[Model-inferred] 令 \(w\) 包含執行前綴及當前全域狀態。定義：

\[
Obs_W(w)=
\langle
\text{W 的初始狀態、持久紀錄、輸入、送出呼叫、收到的訊息、}
\text{所有 API 回傳、timer／clock 讀值、權限通知、隨機選擇}
\rangle .
\]

這包含基準協定保證 W 能看到的全部資訊。S 的 commit、外部效果及「ack 已送出」不在其中，因為協定沒有提供直接觀察它們的介面。

在 A、B 中，安排相同的本地時間及 W 排程：

```text
Intent(k,p)
→ Grant(e)
→ Send(a0)
→ LocalSendAccepted
→ 沒有任何收到的訊息
→ Timeout(a0,T)
```

S 與通道的其他轉移對 W 都不可見。因此：

\[
Obs_W(A)=Obs_W(B)=o_T.
\]

但：

\[
Committed_{a_0}(A)\ne Committed_{a_0}(B).
\]

所以任何只依賴 \(o_T\) 的判定函數，都不能同時正確判定這兩個世界的首次 commit 結果。

這不是把真實協定保證的資訊刪掉後得到的反例：**此處研究的是明確宣告的抽象協定**。若某個實際 API 還提供可查 receipt、可證明的 cancellation、額外通知或去重保證，必須把它們加入觀察與轉移規則，再重新分析。

**4. Safety、liveness 與可採取的決策。**

[Model-inferred] 分開四項要求：

- **不重複效果：** \(\Box(n_k\le1)\)。
- **不遺失已提交操作：** commit 紀錄及未完成義務不得消失；W 不得僅憑 timeout 宣稱「確定未提交」。
- **最終完成／有證據的終止：** 在指定公平性下，操作最終完成且結果可確認，或取得足以支持合法終止的證據。
- **有效權限：** 新的不可逆作用必須在協定指定的生效點接受有效 ownership／retry authority 檢查。

「提交後最終履行效果」是額外的進度要求；durability 本身不會自動提供它。

| W 的選擇 | 在 A、B 是否共同安全？ | 進度限制 |
|---|---|---|
| **直接重試非冪等 Execute** | 否；A 可產生第二次效果 | B 可完成，但不能因此忽略 A |
| **永不重試** | 可以避免新增重複，前提是保留 Unknown／pending 義務 | B 的意圖可能永遠未執行；這是 omission，並非「遺失一個已提交操作」 |
| **等待** | 是；不應因當下無資訊就排除等待 | 若回覆已永久遺失且沒有其他通知，等待本身不能保證 resolution |
| **查詢** | 若有唯讀查詢介面，通常可以安全增加證據 | 基準 API 沒有此能力；查到 Absent 也未必排除在途請求稍後提交 |
| **Reconcile** | 讀取、比對證據可以安全 | 後續修復作用仍需要去重、權限及競態控制 |
| **轉移 ownership** | 在正確交接協定下可以安全 | 新 owner 繼承不確定性；換 owner 不會讓先前效果消失 |

重要例外：即使 status query 是 linearizable，`Absent at t` 也不等於「舊請求以後不會提交」。若舊請求仍在途，查詢後直接發出第二次非冪等呼叫仍可能重複。

[Model-inferred] 可以證明一個**受限的不可能命題**。假設：

1. 沒有去重、查詢、reconciliation 證據來源、取消屏障或自動回覆重送。
2. A 的回覆與 B 的請求已永久遺失。
3. 此後通訊可靠、S 正常且排程公平。
4. 合法操作不能以未經證實的成功／失敗宣告結束。
5. 要求操作最終完成，且從不重複效果。

在 B，若 W 永不再送請求，操作永遠不會完成。因此任何滿足進度要求的策略，必須在某個有限時間重試。

在那之前，A、B 的 W 觀察仍相同，所以同一策略也會在 A 重試。可靠通訊使 S 收到它，按契約再次產生效果，令 \(n_k=2\)。

因此上述五項不能同時滿足。**此證明只需要一次請求或回覆遺失，不需要永久網路分割。** 公平性不會復活已被丟棄的訊息。

若允許永久 `Unknown`、安全暫停或轉人工，則可以保留 safety，但放寬了指定的 completion／resolution 要求。

**5. Mitigation protocols：解決不同的義務。**

[Source-confirmed] 一手資料支持以下區分：

- AWS 將 caller-provided identity、參數一致性及去重紀錄與 mutation 的原子性列為安全重試的重要條件。[AWS Builders’ Library](https://aws.amazon.com/builders-library/making-retries-safe-with-idempotent-APIs/)
- Outbox 保護本地交易與待發事件的關係，仍可能重複投遞；consumer 的效果需要另外處理。[AWS transactional outbox](https://docs.aws.amazon.com/prescriptive-guidance/latest/cloud-design-patterns/transactional-outbox.html)
- Inbox 的去重標記與業務修改需要原子提交；無法參與交易的外部呼叫仍有不確定窗口。[Microsoft idempotent consumer](https://learn.microsoft.com/en-us/azure/architecture/patterns/idempotent-consumer)
- Chubby 的 sequencer 需要由資源接收端檢查，不能只由發送者相信自己持有鎖。[Chubby 論文 §2.4](https://storage.googleapis.com/gweb-research2023-media/pubtools/4444.pdf)

下表是依本模型推導的 **Model-inferred** 比較：

| 協定 | 證據 owner、原子性與保證 | Crash／ownership transfer | 弱化假設的具體反例 |
|---|---|---|---|
| **穩定 key＋durable dedup** | 效果端保存 `(k,p,result)`；唯一性判定與真正效果須具原子關聯，或效果端提供等價冪等原語。可同時支持 safety 與條件式進度 | 新 attempt／owner 沿用 k，讀取同一 durable 紀錄；保存期限涵蓋所有合法重送 | 效果完成後、記 key 前 crash，重試重做；若每次換 k，去重失效 |
| **Durable receipt／status query** | S 保存可由 k 找到的 commit 狀態；若要證明 effect，還需效果端 receipt。查詢增加知識，並非自動提供安全重試 | receipt 必須跨 crash 保留；初始回覆遺失時，k 仍須能定位它 | 查到 Absent 後重試，舊在途請求隨後提交；或把 `CommittedIntent` 誤認為 `EffectDone` |
| **Write-ahead intent／outbox／inbox** | Producer 在同一交易寫業務狀態與 outbox；consumer 在同一交易寫 inbox key 與其本地效果。保留義務並支持重送 | 未完成項目可恢復；relay crash 後允許重送，由 consumer 去重 | Producer commit 後、寫 outbox 前 crash 造成漏送；consumer 發出外部效果後、寫 inbox 前 crash 仍會重複 |
| **Fencing／epochs** | Authority owner 保存單調 epoch；效果資源在 mutation 生效點驗證，與 ownership 更新適當序列化。防止舊 owner 越權 | 新 owner 取得新 epoch，舊請求被擋；durable epoch 不得倒退 | 舊 owner 已合法執行一次，新 owner 在新 epoch 又合法執行同一 k：兩次都通過 fencing，仍重複 |
| **Reconciliation** | 由持有 operation／effect 證據的系統，以 k 比對意圖、commit 與實際效果；修復動作還需並行控制 | 新 owner 接手 durable pending 項目；不確定項目保留至可判定 | 搜尋索引尚未反映已發生效果，reconciler 誤判不存在而重做 |
| **Safe suspension** | W 持久保存 Unknown、k、已知證據及接手責任；不新增未被認證的效果 | crash 後恢復相同 pending 項目；交接前不得遺失責任 | Unknown 只在記憶體，重啟後以新 k 重做；或暫停期限到就無條件重試 |

補充三個界限：

- **Fencing 不等於 deduplication。** 它控制「誰現在有權做」，不回答「同一操作以前是否已做」。
- **Compensation 不等於從未重複。** 抵銷淨結果仍不能抹去曾經發生的兩次可見效果。
- **有限保存期限是契約的一部分。** 例如 Stripe API v1 文件允許在 key 至少保存 24 小時後移除；移除後重用 key 會形成新請求。不能將有限期限保證延伸到任意久的重試。[Stripe idempotent requests](https://docs.stripe.com/api/idempotent_requests)

若通道允許無界延遲，就需要持續保留相關去重證據，或由協定明確拒絕過期請求；單純刪除紀錄後重新接受同一操作不安全。

**6. Belief-state admission 與限定組合命題。**

[Model-inferred] 定義：

\[
B_W(o)=\{w\mid w\text{ 在宣告協定下可達，且 }Obs_W(w)=o\}.
\]

要求 \(B_W(o)\ne\varnothing\)，避免空集合上的虛假認證。

安全判定必須涵蓋動作的遠端後果。不能因為 `SendRetry` 當下尚未增加 F，就判定它安全。對固定後續策略 \(\pi\)，可定義：

\[
Safe(B,\pi)
\iff
\forall w\in B,\;
\forall \rho\in Runs(w,\pi,Env),\;
\rho\models\Box I_{\rm safety}.
\]

准入動作 a，必須有一個以 a 開始、在所有可能世界中安全的後續策略；liveness 再於公平環境下另證。

基準模型中，直接重試對 A 不安全，因此不能通過此認證。**加入協定後，可以改變同一動作在各世界的後果，而不必先消除不確定性。**

一個非循環的限定命題如下。它明確加入基準 API 原本沒有的能力：

| 元件契約 | 可獨立檢查的操作規則 |
|---|---|
| \(C_W\) | 首次發送前持久保存 `(k,p)`；重試／交接保留 k；檢查回覆身分與內容；不把 timeout 當作 abort |
| \(C_S\) | 以唯一約束原子建立 `Intent[k]=Pending` 及待履行項目；重複 k 不建立第二個邏輯操作；不同 p 拒絕；只有收到有效效果 receipt 才標記 Done |
| \(C_E\) | **效果資源內**以單一交易檢查 epoch、檢查唯一 key、修改業務資料並寫入 receipt；已有 k 則回傳原 receipt，不再修改 |
| \(C_R\) | Pending 在 crash 後可恢復；未取得 receipt 不刪除義務；key／receipt 的保存涵蓋全部合法重試 |
| \(C_N\) | 通道不偽造；安全不要求可靠交付，進度另要求重送與處理最終成功 |

此處 E 是 mitigation 額外要求的效果端能力。例如，外部資料庫可把業務 mutation 與 inbox receipt 放在同一交易。**不假設 S 的 intent commit 與 E 的效果交易跨系統原子化。** 任意實體動作或不能參與該原子邊界的外部 API，不自動落入命題。

Assume–guarantee 要求：

\[
M_i\models A_i\Rightarrow G_i,
\]

並證明接線與其他元件的保證確實維持各 \(A_i\)，尤其是穩定 k、receipt 真實性、效果端原子性及 epoch 檢查。

候選結論：

1. 每個 k 至多產生一次指定效果。
2. 每個已 commit 的 intent，持續具有 Pending 義務或有效完成 receipt。
3. 在最終服務可用、權限穩定、公平處理及反覆傳送最終可交付的條件下，操作最終完成並讓 W 得到結果。

歸納證明的核心不是直接假設「exactly once」，而是：

- 唯一 key 的成功插入至多一次。
- 只有該插入所在交易可以產生效果。
- crash 不會留下「效果已提交但同交易 receipt 不存在」的狀態。
- S 即使遺失 E 的回覆，也保留 Pending；重送只會取得既有 receipt。
- 接手者使用相同 k，不能繞過以上限制。

在**從一開始就使用此協定**的 A、B 中，同一個重送動作會：

```text
A：找到既有操作／receipt → 回覆，沒有新效果
B：建立操作 → 執行一次效果 → 回覆
```

不能在沒有 ledger 的舊效果已發生後，事後補上一個 key 就宣稱命題成立。

這區分了三種不同能力：

| 能力 | 是否必要 |
|---|---|
| 知道「現在這個動作在所有可能世界都安全」 | 安全准入需要 |
| 知道「第一次 attempt 在 timeout 前是否 commit」 | 安全重試不一定需要 |
| 協定保證兩世界接受同一動作都安全 | 可以替代事先辨識初始世界 |

W 最後可以知道「k 現在已完成」，而仍不知道究竟是第一次呼叫完成，還是重試後才完成。

**7. 可否證預測。**

以下均為 **Model-inferred**，本輪未執行：

| 預測 | 否證條件／必要重審 |
|---|---|
| 基準 A、B 的 W 觀察相同 | 若宣告 API 必定暴露不同 receipt／通知，就必須納入 Obs，重做證明 |
| 無去重重試可使 A 的效果次數變成 2 | 若每次重送都安全，檢查是否其實存在去重或自然冪等保證 |
| intent commit 後可尚無外部效果 | 若實作有共同原子交易，該中間世界不適用該實作 |
| atomic inbox＋效果 mutation 能跨 crash 防止重複 | 任一合法 crash trace 出現 effect 已提交、同交易 receipt 卻不存在，即否證原子性或模型映射 |
| fencing alone 不能防止不同合法 epoch 重做 k | 若重做被拒，須識別額外的 operation dedup／業務唯一約束 |
| status 的 Absent 不能單獨排除晚到提交 | 若 Absent 同時建立取消或拒絕舊 attempt 的屏障，協定已比基準更強 |
| 安全暫停不保證最終 resolution | 若有必達的 recovery／通知機制，應將它列入進度假設 |

**8. EGTS／CCPO v0.4 的整合建議。**

[Model-inferred] 建議新增「durable outcome 與知識」維度，保留既有 causal 模型：

- 全域分析狀態包含 \(D_{\rm actual}\)、\(F_{\rm actual}\)、ack sent／received、在途請求及實際 authority。
- Worker 狀態只包含它取得的 commit／effect／authority 證據與 belief state；不得讓 policy 直接讀取不可見的全域 D。
- Evidence 綁定 `(operation, payload, attempt, scope, epoch, owner, retention/validity)`。
- 將 **settled、intent committed、effect occurred、receipt recorded、ack received** 設為不同事件。
- `NeedEvidence` 的合法處置包括 query、reconcile、受保護重試、suspend 與 handoff，不只 fetch。
- 將「增加知識」與「透過協定使未知結果不再妨礙安全動作」分開。
- 進度義務記錄 recovery owner、可用協定、需要的公平性，以及在永久故障下允許的終止方式。

既有因果順序義務仍必要：去重無法修正使用錯誤語言狀態產生的命令，fencing 也無法修正錯誤的 Update 交付順序。C-1 增加的是完成這些前置工作之後，跨 durable boundary 與通訊不確定性的義務。

要套用到 Temporal，仍需選定具體操作，核對其 identity、commit 點、receipt、重試／去重範圍、ownership 檢查與外部效果端契約，並構造該實作下真正可達且完整觀察相同的兩條路徑。本輪沒有進行這項實作調查。

**9. 已建立與尚未建立。**

- **已建立〔Model-inferred〕：** 在明列模型內，A、B 都可達且對 W 不可區分；無保護重試不能普遍安全；在缺少額外協定能力的限制下，不重複效果與最終完成不能同時保證。
- **已建立〔Source-confirmed〕：** 去重、outbox/inbox、receipt query 與 fencing 各自依賴不同的證據與原子性條件，不能互相替代。
- **限定命題〔Model-inferred〕：** 穩定 identity、durable pending 義務、效果端原子去重及有效權限，可以讓相同重試在兩世界安全，無須先查明首次 commit 結果。
- **未建立〔Unknown〕：** 任意外部效果的 exactly-once、永久故障下的無條件進度、實際 Temporal 的不可區分世界，以及任何具體 Temporal ambiguous-commit bug。

**本輪確立的是受限協定的資訊與能力限制：局部完全守約仍可能留下 commit uncertainty；它可能阻止安全進度，也可能被額外協定安全地容忍，而不必先消除。** 停止於 C-1。
