C2-E2 研究證據與重現

閱讀 REPORT.zh-TW.txt、predictions.before-run.json、results.json、activation-timeline.json。
原始run分別在scanner-01、scanner-02、semantic-h0-01、semantic-hs-01；每個目錄有stdout.log、stderr.log、exit-code.txt與invocation.json。有效runs另有protobuf、source snapshot與精確test-only.diff。fixture-hashes.json列四個history身份。

來源：checkpoint 899ccfbfbf59d80797dc0274ff6c20c96e608f65，production parent 8f4a676f9e1a6276b5cc2f77e25c9183ce256fe9。
本工作區尚未commit。重建另一個獨立checkout時，在checkpoint上套用本目錄test-only.diff，並保留本evidence目錄及checkpoint原有C2-D evidence。不要在舊research workspace原地套用。

Prerequisites：Linux Rust 1.94（本次1.94.1）、Cargo、protoc（本次3.21.12）、compiler/linker、對應cached dependencies及數GB空間。不需Server。Cargo.lock被repo忽略追蹤，獨立clone不會有；若不存在，複製本目錄dependency.Cargo.lock到新repo root的Cargo.lock。不要覆寫使用者已有lockfile；先核對。此檔原件／hash與15份matching manifests見dependency-lock.provenance.json。

在本repo root、使用全新尚不存在的run名稱，依次執行：
python3 research-evidence/c2e2-signal-boundary-2026-10-22/run-focused.py scanner scanner-NEW
（只有exit0才繼續）
python3 research-evidence/c2e2-signal-boundary-2026-10-22/run-focused.py H0 semantic-h0-NEW
（只有exit0才繼續）
python3 research-evidence/c2e2-signal-boundary-2026-10-22/run-focused.py Hs semantic-hs-NEW

wrapper使用--locked --offline及--exact --nocapture，只執行指定unit test；不做額外cargo build。C2E2_BUILD_DIR可指定新的Cargo cache；預設沿用既存__BUILD_CACHE__，這是build cache，不是research evidence。
wrapper以create-new輸出目錄避免覆寫。原始run的缺lockfile失敗保留；後續使用同一預測。原C2-D test不被執行。

H0 scanner input是checkpoint bundled E2-D精確bytes。Hs為另外的Signal衍生fixture。Runtime H0/Hs各自保存warm實際marker／metadata，再供該次cold使用。Worker-clock original_schedule_time與protobuf map/flag順序可能使重跑hash不同；不允許由此放寬clock、LA identity/result、activation group、command或正常完成assertions。

本見證限單一無await Signal handler、單一pending LA、deterministic LA substitute，沒有production changes、Server E2E、GitHub writes或general equivalence宣稱。
