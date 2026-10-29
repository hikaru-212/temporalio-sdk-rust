# D-2 驗證限制

1. **有限模型與無界系統。** 窮舉只涵蓋固定 K、兩個 attempts、epoch1→2、單一 certificate。書面歸納 argument 可在相同規則下參數化，但不是 proof-assistant machine proof，也沒有證明實作模擬該模型。
2. **不是 SQLite simulator。** TryMutation 是 private write-set；模型不取得SQLite writer lock，不模擬整個資料庫 snapshot version／BUSY_SNAPSHOT。部分抽象排程是比實體SQLite更寬的行為集合；三條 witness可嵌入不證明全部抽象路徑都可實現。
3. **拆分契約不等於可分離的物理機制。** SQLite全域 snapshot conflict可能同時保護 claim與epoch；PostgreSQL鎖、Cassandra條件寫不能視為相同。A/B反例只說拆分介面前提有獨立角色，沒有真實backend RED。
4. **claim bundle粒度。** A同時停用一個 commit-time live claim predicate內的pending/revision條件；這符合「移除原子claim重驗」，但不能據此稱兩子條件各自必要。B停用同一epoch-validity契約的兩檢查點；commit檢查足以提供該安全性，mutation檢查主要提前拒絕。
5. **P2(b) 的讀取範圍。** monitor禁止certificate之後舊request新增有效state，不禁止新epoch合法修改、不把晚回覆當晚提交。完整tuple freshness由模型規則及frame audit固定，不是本輪弱化檢驗的變量。
6. **logical ledger。** H只是physical append的抽象；E/Q是effective batch與command-generated task的邏輯receipt，不是實際history branch、ActivityInfo或六類task row。任一資料列額外unique constraint都可能補上弱模型欠缺的保護。
7. **revision非實體逐byte模擬。** 模型每次接受原子v+1；A可到v5。實際SQL可能以request計算的新revision賦值；現有C-5B polling也會升v5。接受次數由Commit ledger獨立計算，不能用revision數值單獨推斷。
8. **unknown／ACK。** 模型區分commit、manager result、ACK送出/收到/丟失、caller timeout；但沒有完整Worker觀察函數或所有protocol evidence。既有controlled manager error不是native DB commit-timeout，模型也未將它升格。
9. **liveness。** 只證安全性並展示合法續行路徑；沒有公平性、無限重試、超時失效或successor生命週期，因此不保證所有執行eventual completion。有限模型結束不等於服務具備一般進度。
10. **不包含的語義。** 無語言settlement、Signal/Update、workflow決策、外部Activity effect、task retirement／跨owner移交、crash recovery、multi-cluster、K reuse、無界authority epochs。未重證#1597或#1629，未擴張Temporal調查。
11. **可信基礎。** 檢查器與Python interpreter不是經形式驗證的kernel；保存來源、原始結果、最短trace與獨立紙上最短下界供審查。沒有額外第三項ablation。source/規格/預測皆有探索前hash；來源未變核對不等於新的產品runtime證據。
12. **不主張理論新穎性。** 核心推理是標準atomicity/CAS/fencing；可主張的是此CCPO模型內的分離、冗餘和映射義務精確化。尚無general Temporal theorem或external exactly-once。
