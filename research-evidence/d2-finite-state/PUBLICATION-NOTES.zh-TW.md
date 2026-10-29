# D-2 公開候選：出版與保全說明

本次只準備候選，沒有重跑／修改模型、重做 Temporal 實驗或建立 commit/push。本包明列的結果是已完成 D-2 run-01 的歷史結果，並非本次公開準備的新 PASS。

## 保留與轉換

- 三個 Python 模型／explorer source、數學轉移規格、限制、反例表、所有探索 result JSON、完整六條 trace、stdout/stderr 及 exit 紀錄均逐位元組保留。
- `PREREGISTRATION.public.json` 只把原本的 D-1 私有絕對路徑替換為 `D1_ORIGINAL_REPORT`；預測、原報告 hash 及所有其他 bytes 不變。它是執行前原稿的公開副本，不把去識別化之後的 hash 假稱執行前 hash。
- `run-01/invocation.public.json` 只正規化 argv/cwd 的證據根目錄，保留開始時間、timeout、source hashes、原始 preregistration hash，並新增轉換說明；命令是歷史紀錄，不能直接對現有結果路徑重跑。
- `REPORT.zh-TW.md` 是明確標記的公開副本：替換本機路徑／連結、移除裝置識別、改用公開來源索引及去識別化保全摘要。數學論證、數量、反例、限制、必要性／冗餘分析不改。
- README 重寫為可攜操作說明。另附 D-1 第7／9節精確摘錄，原始行號與 hashes 可查。
- `provenance/original-SHA256SUMS` 與 `original-PRE-RUN-SHA256SUMS` 僅是原始封存的雜湊紀錄；其中故意未公開的本機檔案不會出現在此包。公開包的完整性應以根目錄 `SHA256SUMS` 驗證。

## 歷史 audit 差異

原始嚴格保全 audit 退出1：它先通過來源／trace檢查，之後發現一個新增的工具內部 Git capture ref。原有研究 refs、HEAD、branch、工作目錄狀態、tracked diff 未變；後續 audit 明列該差異並退出0，而不是宣稱所有 refs 完全不變。

此公開摘要保留失敗與限定結論。完整本機 repo paths、capture ref識別碼／物件、Git指紋、audit工具與原始stderr留在原始證據；不公開內部metadata，也不刪除歷史失敗。原檔 hashes及排除理由逐檔記在 `ORIGINAL-TO-PUBLIC.json`。沒有完整session或session備份進入本包。

## 公開審核與重現程度

原始37項manifest及額外manifest本身已核對；模型source與run-01記錄的hash相同。公開檔案經明確allowlist、UTF-8文字檢視、私有路徑／金鑰／常見token／session與內部ref掃描。這不是能偵測所有形式秘密的安全證明；本包沒有需公開的憑證、環境設定或未審查binary。

獨立重現的判定是：三個原始source完全相同、imports閉包只有本包model/properties與Python標準函式庫、模型不讀私有工作目錄。公開準備只執行保存檔案的hash／JSON／AST核對，不import、compile或execute模型。未對公開副本另做新runtime驗證。

數學範圍仍限固定K、兩attempts、epochs1/2與明列契約。bounded exhaustive checking不等於無界定理或Temporal implementation verification。既有SQLite三個有限witness不是全部backend的fencing定理，也不證native DB timeout、crash recovery、外部exactly-once或一般liveness。
