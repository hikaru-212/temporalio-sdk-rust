C2-E2 研究 checkpoint

分支：research/ccpo-egts-c2e2
Parent：899ccfbfbf59d80797dc0274ff6c20c96e608f65
Production base：8f4a676f9e1a6276b5cc2f77e25c9183ce256fe9

這是受控 scanner 與真實 Rust Guest／WorkflowManager 的研究見證，不是 production repair，也不是完整 Server reachability 或一般 semantic refinement 證明。

保存的結果
- scanner-01：exit101，gitignored Cargo.lock 缺失，編譯前停止；環境／harness INCONCLUSIVE，不是 correctness RED。
- scanner-02：exit0，H0 after3→event9；Hs after3→event7、after7→event10。
- semantic-h0-01／semantic-hs-01：各exit0，兩路正常完成，使用相同P_sig與expanded Guest adapter。
- Hs Signal兩路各一次；真實StartedRoutine→poll→Signal(Ok)→MAIN Blocked→空commands。
- Hs Signal讀值T+8/history_length7，LA resume T+17/history_length10。H0沒有Signal，保留原有clock/history-length收斂。
- 每個family各自比較warm/cold，沒有warm(H0)對cold(Hs)。所有raw traces與差異均保存。

限制
這是單一固定程式、單一pending LA、無await probe handler及確定性LA executor替身。Warm callback一次，cold無新增New LA request，不能外推真實外部系統exactly-once。Warm/cold初始化mode、deployment與SDK metadata不同；既有差異早於folding，不隱去也不宣稱完整activation或所有讀值相等。沒有完整Worker transport、paginator/EOF authority、Server durable commit、Server生成此交錯、failure recovery或一般refinement證明。Non-silent步被保留並匹配的PASS，不證明C1對所有refinement必要或C1-C5普遍充分。

重現
使用此研究分支的完整checkout。兩個新增test-only檔案與module宣告已在Git中，不需重套test-only.diff。
Prerequisites：Python3.9+、Git、rust-toolchain.toml指定Rust1.94（原執行1.94.1）、Cargo、Linux compiler/linker、protoc（原執行3.21.12）、數GB建置空間，以及Cargo dependencies cache或允許下載。不需Server或GitHub credentials。

唯讀靜態檢查：
python3 research-evidence/c2e2-signal-boundary-2026-10-22/reproduce.py --verify-only

需要重跑時，選擇尚不存在的輸出目錄：
python3 research-evidence/c2e2-signal-boundary-2026-10-22/reproduce.py --output /tmp/c2e2-reproduction-NEW

Wrapper依scanner→H0→Hs順序執行，只在前一階段exit0才繼續，保留--exact --nocapture。預設--locked --offline；若dependency cache不足可明確加--allow-network，仍保留lockfile。預設build cache位於新output/build，可用--build-dir指定其他cache。每階段保存stdout/stderr、exitcode、命令、環境覆寫、source snapshot及fixture hashes。它拒絕覆寫既存output或在archived evidence內輸出。
Cargo.lock被原repository忽略追蹤。若root lockfile不存在，wrapper以exclusive create複製bundled dependency.Cargo.lock；若已有不同lockfile，停止而不覆寫。原lock hash與來源核對記錄在dependency-lock.provenance.json。verify-only不建立lockfile、不編譯、不跑test。

Immutable inputs
inputs/E2-D.pb是原C2-D fixture精確副本，SHA-256：68de8fb53e38036a3cf155ce9d63169e0bad4d371ca4fbc2a95df4d060a7e1df。無需任何舊temporary worktree或未追蹤fixture。原C2-D輸入亦隨parent checkpoint保留。
H0/Hs runtime fixtures另有身份；actual warm marker與metadata先凍結，再供同一family的cold使用。Worker-clock original_schedule_time與protobuf map/flag順序可能使重跑hash不同；不放寬identity/result/clock/group/command/正常完成assertions。fixture-hashes.json保存歷史樣本精確hash。

證據與公開副本
REPORT.zh-TW.txt、README.original.txt及historical-runner.txt記錄原實驗時點，當時尚未commit。historical-runner.txt是provenance，含路徑placeholders，請勿當作重現腳本；應使用reproduce.py。
原始workspace、raw logs、預登記與manifest均留在本機不變。本分支為另外建立的公開副本。publication-map.json逐檔記錄original/public路徑、轉換及兩種SHA-256；只遮蔽不必要本機路徑。所有protobuf、測試程式及實驗觀察值未變。
original-SHA256SUMS.json及各run內同名檔案保存原manifest；SHA256SUMS.json是公開檔案的manifest。歷史invocation中的predictions_sha256仍指原始預登記，對應publication-map的original_sha256；公開預登記只替換路徑，沒有事後修改預測。
沒有加入build cache、target、無關研究或credential資料。完整新增／修改檔名見COMMITTED_FILES.txt。

本checkpoint準備期間沒有重新編譯或執行研究測試。新版portable wrapper經靜態檢查與verify-only，尚未聲稱新的環境runtime PASS；原scanner/H0/Hs PASS證據完整保留。
