# CCPO / EGTS D-2：可獨立審核的公開研究候選

此包保存已完成的有限狀態探索；本次公開準備沒有修改或重跑模型。從 [研究報告](REPORT.zh-TW.md) 與 [完整轉移規格](MODEL.zh-TW.md) 開始閱讀。

| 配置 | reachable states | P2(a) | P2(b) |
|---|---:|---|---|
| baseline | 23,385 | PASS | PASS |
| A：移除commit-time claim | 23,105 | 最短12步反例 | PASS |
| B：移除epoch read-set conflict | 17,371 | PASS | 最短10步反例 |

這是固定K／兩attempts／兩epochs的bounded exhaustive exploration。書面歸納草圖不等於已機械證明的無界定理；本包不是Temporal implementation verification。A/B不是Temporal correctness RED。

## 內容與證據

- `source/{model,properties,explore}.py`：三個原始模型source，與歷史run的hash完全一致。
- `MODEL.zh-TW.md`、`PREREGISTRATION.public.json`、`REPORT.zh-TW.md`、`LIMITATIONS.zh-TW.md`：規格、原預測的路徑去識別化副本、歸納／冗餘分析及限制。
- `run-01/results/`：三配置完整結果、兩條最短反例、Main/Positive/Negative與ACK-loss四條抽象witness。不是四個新Temporal實驗。
- `run-01/stdout.log`、`stderr.log`、`exit.json`：原始bytes；`invocation.public.json`正規化本機路徑並標明轉換。退出碼0，stderr為空。
- `provenance/`、`PROVENANCE.json`、`ORIGINAL-TO-PUBLIC.json`：原始hash、D-1範圍摘錄、逐檔公開／排除決策與hash對照。
- `PUBLICATION-NOTES.zh-TW.md`：所有去識別化類型、歷史audit失敗與後續限定結論、靜態重現審核界線。
- `PUBLICATION-ALLOWLIST.json`：唯一允許納入的repository-relative檔案清單；本次尚未stage。
- `tools/verify_public.py`：只讀保存檔案的hash/JSON/AST verifier，不import或執行模型。

## 不重跑模型的公開包核對

Python ≥3.10，僅標準函式庫。從此包的根目錄執行：

```bash
python3 -B tools/verify_public.py
```

核對allowlist、manifest、原始source hashes、保存結果／stdout一致性、trace計數及靜態imports閉包。它不重新列舉reachable states，不能用它取代explorer。原始manifest位於provenance，含故意不公開的本機檔案名稱及hash；不要把它當成公開包的完整檔案清單。

## 未來獨立重現探索

以下是提供給審核者的指令，**本次準備未執行**。Python 3.12.3是歷史執行版本；無第三方依賴、網路、Temporal checkout、Go cache、原始私有workspace或環境設定需求。

```bash
python3 -B source/explore.py --output ../d2-reproduction-NEW
```

輸出目錄必須不存在，且應放在此凍結公開包之外；程式拒絕覆寫。不要使用歷史invocation的`run-01/results`路徑重跑。可自行在外層設定資源上限；歷史執行使用600秒timeout且沒有觸發。被中止的探索不代表queue已耗盡，也不能報PASS。

同一source與配置應產生相同state/edge數、性質與最短長度；wall time不預期一致。輸出source hashes由source目錄的Python檔計算，故不可在其中放額外helper。模型中的v5重複接受不等於既有Temporal Activity polling造成的v5。

## 來源與限制

研究Git parent為 `c86de99134b3a1d4144e089d6c1bba35e29debcc`；[既有C-5B證據](https://github.com/hikaru-212/temporalio-sdk-rust/tree/c86de99134b3a1d4144e089d6c1bba35e29debcc/research-evidence/c5b-range-fencing)供映射參考，執行模型不依賴它。D-1原報告hash與精確摘錄見PROVENANCE。

靜態依賴與byte完整性已核對；此公開副本沒有另做新runtime驗證。沒有native DB timeout、crash、所有backend、外部exactly-once或一般liveness證明。此候選不包含完整session、本機Git指紋或未審核binary。尚未commit/push；任何發布須另獲授權。
