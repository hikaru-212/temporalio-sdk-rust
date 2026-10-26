公開去敏與完整性規則 v1

原始證據未原地修改。本包是另製公開候選；只有allowlist明列的files可進下一個commit。沒有完整Codex transcripts、session backups、session-excerpts、原始環境inventory、cache、DB或protobuf binaries。新Go source與兩份diff是exact copies，不變更測試邏輯或格式。

`PUBLIC-TRANSFORMATIONS.json`逐檔列出original bundle label／relative path／SHA-256、public path／SHA-256及具體規則。`runs/<arm>/transformation-actions.json`記錄每個被處理的JSON位置、規則、original value hash與必要alias；不含被移除的原始秘密值。`tools/sanitization.py`為純轉換程式，方便獲授權者用原件檢查轉換；公開讀者不需要私人原件也能用tools/verify_public.py核對公開內部一致性。

規則：

1. 保留全部CCPO研究event，原sequence、UTC、stdout line來源不變；error字串／type、RangeID、revision、WFT因果參照與task IDs保留。完整Server configuration／非研究輸出不進stdout.research-projection.log，原始行總數／保留行號／省略數見result.public.json。此projection不冒稱完整raw stdout。
2. 保留測試生成的namespace/run/workflow UUID，這些不是使用者個資或認證secret，供跨觀察因果對照。同一arm的transaction／connection pointers改成一致代號；它們本來就不是durable database身份。三個arm不共用alias namespace。
3. packed task token及token_hex全部withheld，僅保留原value hash；parsed task identity保留scheduled/started/attempt/run/clock語意。opaque branch token與pagination token改成一致alias，保留相等關係、分頁串接與EOF。alias mapping只公開原值hash，不公開原encoded bytes。
4. 原SQL row的Data／protobuf blobs及完整normalized internal request不公開。保留可讀request shape、RangeID、revision、history event IDs及ownership digest證據；internal serialized value只留hash。資料庫row中16-byte synthetic UUID的base64轉成標準UUID文字，公開記錄轉換hash。普通synthetic payload如ccpo-input仍保留。
5. stack只保留function及來源位置；pinned Server位置改為repository-relative路徑，機器home/cache路徑與本機endpoint刪除，argument addresses／goroutine numbers不保留。SQL配置只列PluginName、ConnectAttributes與pool size等研究欄位，omit User/Password/TLS/connect endpoints。DB位置以arm-specific代號表示。
6. invocation採嚴格environment白名單及具名路徑placeholder；沒有完整PATH。原始result的exit／timestamps保留，completed-audit classification另標來源；沒有回寫私有raw result中的pending-analysis欄位。
7. 每份未公開protobuf都有原始hash／type／bytes與對應的runtime JSON／觀察位置。這些是語意投影，不是重新decode或re-encode產生的「原始binary」。shard/describe只公開必要authority/MS觀察；activity-token沒有完整decoded公開等價物，限制明列。
8. predictions原始正文完整保存，只加上來源註記；正文中「當時controls尚未執行」不追改。主報告是新整理的完成後結論，歷史C-5B.1 partial recovery及lint failure不升格成新PASS。

hash coverage無循環：PUBLIC-TRANSFORMATIONS覆蓋所有payload；PUBLICATION-ALLOWLIST列所有檔名；SHA256SUMS覆蓋payload、mapping與allowlist但不含自身。manifest自身hash由checkpoint準備紀錄／最終回報保存。原始manifest hash是保存承諾，不是公開payload的manifest。去敏後hash自然不同，不得混用。

安全審閱包含：逐檔allowlist、known removed raw值不得殘留、local path／credential模式掃描、所有事件數及critical safety欄位守恆、分頁EOF與task identity核對。這不是任意encoded秘密不存在的數學證明。被省略內容不可由public bundle還原；對原bytes的獨立逐byte比對仍需私人原件授權。public candidate本身不包含該授權或任何credentials。
