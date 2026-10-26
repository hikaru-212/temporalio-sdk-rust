C-5B.2 三組受控結果比較（2026-10-26原始runs；本次未重跑）

| 比較項目 | Main | Positive | Negative |
| --- | --- | --- | --- |
| Transaction ordering | Tx讀r1→pre-mutation gate→early error→renew r2→fresh read→同Tx仍讀r1→release→真正mutation | Tx讀r1→真正mutation及commit→delegate done→early error→renew r2→fresh read | early error時實際delegate尚未啟動→renew r2→fresh read→才啟動帶r1的request→SQL讀r2 |
| RangeID | 獨立持久讀1→2；old snapshot保持1 | commit時1，之後獨立持久讀2 | 啟動舊request前已2；舊request之實際讀亦2 |
| SQL transaction結果 | `*sqlite.Error`、517／BUSY_SNAPSHOT；commit0、rollback1 | 真正Commit=nil；commit1、rollback0 | 真正 `*persistence.ShardOwnershipLostError`；commit0、rollback1；未進mutation gate |
| Mutable State revision | 3→barrier3→old abort後3→合法retry4→poll5 | 初始3→commit4→barrier4→rejected retry仍4→poll5 | 3→barrier3→stale reject仍3→合法retry4→poll5 |
| Effective history | release前後events1–3相同；retry後唯一Completed4及ActivityScheduled5 | barrier後events1–5；retry前後bytes相同，沒有第二個接受 | stale前後events1–3相同；retry後唯一Completed4及ActivityScheduled5 |
| 六類durable tasks | release前後ledger相同；retry後唯一Activity transfer2097156/event5 | rejected retry前後ledger相同；唯一Activity transfer1048591/event5 | stale前後ledger相同；retry後唯一Activity transfer2097156/event5 |
| Retry／可續行 | 正常同request接受一次；Activity polling成功 | 同request回NotFound；獨立state確認原接受仍在；Activity polling成功 | 正常同request接受一次；Activity polling成功 |
| Cleanup | done後讀結果；Tx rollback/closure確認；兩connections之後關閉；registry0 | done後讀結果；Tx commit確認；兩connections之後關閉；registry0 | done後讀結果；Tx rollback/closure確認；兩connections之後關閉；registry0 |

三組使用相同test source、獨立DB/workflow。mutation是首個SQL mutation hook的計數，非整個transaction的SQL statement數。RangeID2是authority certificate，不是原WFT的commit receipt。
