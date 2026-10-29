# D-2 最短反例逐步表

此表是已保存原始 JSON 的語意投影；沒有新增探索或改寫原 trace。完整 request、snapshot、caller/manager/ACK、monitor 均在原 JSON。

## A_no_commit_claim

原始：[JSON](run-01/results/A_no_commit_claim/counterexample-P2a.json)；最短 12 個模型轉移。

| 步 | 動作 | epoch | revision | pending | effective batches／Q | certificate | 性質 |
|---:|---|---:|---:|---|---|---|---|
| 0 | Initial | 1 | 3 | True | [] | none | P2a=True |
| 1 | Submit(0) / first | 1 | 3 | True | [] | none | P2a=True |
| 2 | AppendPhysical(0) | 1 | 3 | True | [] | none | P2a=True |
| 3 | Begin(0) | 1 | 3 | True | [] | none | P2a=True |
| 4 | ReadSnapshot(0) / validated | 1 | 3 | True | [] | none | P2a=True |
| 5 | TryMutation(0) / private_write_set | 1 | 3 | True | [] | none | P2a=True |
| 6 | Submit(1) / concurrent | 1 | 3 | True | [] | none | P2a=True |
| 7 | AppendPhysical(1) | 1 | 3 | True | [] | none | P2a=True |
| 8 | Begin(1) | 1 | 3 | True | [] | none | P2a=True |
| 9 | ReadSnapshot(1) / validated | 1 | 3 | True | [] | none | P2a=True |
| 10 | Commit(0) | 1 | 4 | False | [0] | none | P2a=True |
| 11 | TryMutation(1) / private_write_set | 1 | 4 | False | [0] | none | P2a=True |
| 12 | Commit(1) | 1 | 5 | False | [0, 1] | none | P2a=False |

## B_no_epoch_conflict

原始：[JSON](run-01/results/B_no_epoch_conflict/counterexample-P2b.json)；最短 10 個模型轉移。

| 步 | 動作 | epoch | revision | pending | effective batches／Q | certificate | 性質 |
|---:|---|---:|---:|---|---|---|---|
| 0 | Initial | 1 | 3 | True | [] | none | P2b=True |
| 1 | Submit(0) / first | 1 | 3 | True | [] | none | P2b=True |
| 2 | AppendPhysical(0) | 1 | 3 | True | [] | none | P2b=True |
| 3 | Begin(0) | 1 | 3 | True | [] | none | P2b=True |
| 4 | ReadSnapshot(0) / validated | 1 | 3 | True | [] | none | P2b=True |
| 5 | TryMutation(0) / private_write_set | 1 | 3 | True | [] | none | P2b=True |
| 6 | RenewBegin | 1 | 3 | True | [] | none | P2b=True |
| 7 | RenewCommit | 2 | 3 | True | [] | none | P2b=True |
| 8 | Acquire | 2 | 3 | True | [] | none | P2b=True |
| 9 | CertifyFresh | 2 | 3 | True | [] | r2/v3/pending=True | P2b=True |
| 10 | Commit(0) | 2 | 4 | False | [0] | r2/v3/pending=True | P2b=False |

A 最後一步 baseline 會 claim_conflict_at_commit；B 最後一步 baseline 會 epoch_conflict_at_commit。A 最短長度下界為兩次六步接受；B 為一次六步接受加四步 authority/certification。

B 的 TryMutation 是私有 write-set，不是宣稱 SQLite 實體 mutation 後可讓另一 writer renewal commit。兩條都是弱化契約的抽象反例，不是 Temporal correctness RED。
