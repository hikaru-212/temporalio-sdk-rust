CCPO C-5B：受控 SQLite RangeID fencing 研究 checkpoint 候選

本包保存2026-10-26已完成的 C-5B.2 Main、Positive、Negative。三組皆PASS；本次只做公開證據準備，未編譯或重跑。結論為 **CONFIRMED SAFETY，僅限這三個受控 SQLite／normal WFT 排程**，不是一般Temporal correctness theorem、native database commit-timeout重現或外部副作用exactly-once。

此為Temporal Server Go實驗，保存於SDK-Rust研究fork；不是SDK-Rust或Server production patch。

- Server source：`0d5e3848bed625907987f73d5eaae8dc77bc6590`。
- SDK研究Git parent：`ea6507ed48614f71a7426740b61c51029f3c9541`。
- 候選分支：`research/ccpo-egts-c5b-fencing`，目的地僅 `hikaru-212/temporalio-sdk-rust`。
- 本階段沒有commit、push、PR、Issue或comment。

閱讀順序：[最終報告](FINAL-RESEARCH-REPORT.zh-TW.md)、[三組比較](COMPARISON.zh-TW.md)、[方法與scope](METHODOLOGY.zh-TW.md)、[重現條件](REPRODUCTION.zh-TW.md)、[去敏規則](SANITIZATION.zh-TW.md)。歷史Phase0／C-5B.1與P2前source見[歷史限制](historical/RECOVERY-LIMITATIONS.zh-TW.md)，不得標成這次的新實驗。

`source/tests/ccpo_c5b2/`三檔及`patches/test-only.diff`與三個PASS run完全相同。`lineage/original-p2-defect/`是已知cleanup缺陷的舊source存檔；不要拿它執行或把P2 patch再套到已修正來源。

每個`runs/<arm>/`保留全部CCPO研究事件的去敏投影、原始行號／UTC、有效State/history、全部六類task分頁、原始invocation的正規化副本、exit0與原始binary hashes。`stdout.research-projection.log`保留所有研究事件及Go結果行，省略其他Server輸出，明確不是完整raw stdout。未公開任何protobuf binary或完整session。

離線驗證本包（只讀檔案、驗hash與已保存的觀察，不啟動Temporal或執行實驗）：

```sh
python3 tools/verify_public.py
sha256sum -c SHA256SUMS
```

`PUBLICATION-ALLOWLIST.json`列出預定commit的全部檔名；不得用整個worktree的`git add -A`。`PUBLIC-TRANSFORMATIONS.json`記錄payload原始／公開hash及轉換規則；allowlist與mapping自身由`SHA256SUMS`覆蓋，manifest不自稱包含自身hash。

限制：公開觀察可獨立檢查時序、有效接受數及task分頁，但不能從hash反推出私有raw bytes，也不能僅靠hash證明runtime觀察真實性。對原始至公開轉換的逐byte再驗，需要獲授權存取私人原件。原件未改寫或刪除。
