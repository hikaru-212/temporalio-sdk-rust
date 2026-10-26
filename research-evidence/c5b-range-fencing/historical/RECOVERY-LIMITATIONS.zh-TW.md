歷史恢復範圍與P2 lineage

此附錄不代表新執行。Phase0與C-5B.1原暫存workspace消失後，source由原始command-output紀錄恢復；7份source符合歷史SHA。這裡保存其中Phase0/C-5B.1四份source的exact bytes（.go.txt archive），以及lineage目錄中的P2前三份C-5B.2source；不包含完整sessions或其excerpts。

歷史C-5B.1 run01/02 exit1（INCONCLUSIVE）、run03/04 exit0的紀錄部分恢復；lint preparation exit2，不能稱lint PASS。歷史Phase0有PASS／517紀錄，但完整raw logs、DB、protobuf與部分原manifest未找回。這些缺口標為PARTIAL RECOVERY／MISSING，未合成補檔。

目前source/tests/ccpo_c5b2是經獨立P2 correction後、2026-10-26 Main/Positive/Negative三組共同執行的exact來源。P2只修正cleanup termination確認及connection teardown guard；ownership_test.go未改。patches/p2-only.diff只供lineage審查，不能再套到目前source。lineage hash清單的basename屬原始存放格式；檔案在public archive增加.txt副檔名，bytes未改。

新三組run各有完整指定本地evidence包；公開版本是受控去敏projection，並非所有原始bytes公開。不得把新PASS標成舊B1驗證，也不得把歷史PASS或lint failure改標成這次execution。
