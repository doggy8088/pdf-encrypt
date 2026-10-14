---
name: bump-and-release
description: 為 pdf-encrypt 專案提升版本，透過既有 GitHub Actions 發行並驗證結果。用於要求 bump-and-release、升版發行或重發既有版本時；只要求修改版本或預覽流程時，依指定範圍準備，不觸發發布。
---

# bump-and-release

**沿用專案既有發行機制，完成版本同步、檢查、提交、發布與結果驗證。**

使用者明確要求發行時，可在既有授權範圍內完成提交與推送。只要求建立技能、規劃或本機升版時，停在該範圍。需要補充資訊時，以正體中文詢問具體缺少的資訊；已取得的授權不重複詢問。

* * *

## 確認發行範圍與版本

1. 從 Git 根目錄操作，先讀取適用的 `AGENTS.md`、`Cargo.toml`、`Makefile`、`.github/workflows/release.yml` 與 `dist-workspace.toml`。確認套件為 `pdf-encrypt`，以目前檔案內容為準。
2. 檢查工作分支、remote、`git status --short`、已暫存及未暫存差異。保留現有修改；只納入使用者授權的功能、修正及發行內容，範圍不明時先詢問。
3. 查詢遠端分支、標籤與 GitHub Release，確認發行基準及目標版本是否已存在。可使用 `git fetch origin --tags`、`git ls-remote`、`gh release list` 與 `gh release view`。先從 remote 確認 GitHub 儲存庫，後續 `gh` 指令明確指定 `--repo`。驗證失敗時，區分不存在、驗證身分失敗及網路錯誤，不能一律視為尚未發布。
4. 使用者指定精確版本時，以該版本為準。否則依指定的 `major`、`minor`、`patch` 決定目標，未指定時採用下一個 patch 版本。若 `Cargo.toml` 已包含高於最近發行版、且符合本次要求的待發布版本，沿用該版本；工作目錄已準備好的升版也適用，避免再次加一。從差異與提交紀錄確認來源，版本意圖不明時先詢問。新版本必須符合 SemVer，標籤採 `v<version>`。
5. 確認預定發行提交包含遠端 `main` 的既有內容。若發生分歧，先整合並重新驗證；有未提交修改時，使用隔離工作樹或其他能保留修改的方法處理。若目前不是 `main`，依使用者已指定的 PR 或合併流程準備；缺少合併方式時，完成本機準備後再詢問，不直接將其他分支推成 `main`。

* * *

## 同步版本與驗證

- 更新 `Cargo.toml` 的 `[package].version`，同步 `Cargo.lock` 中名稱為 `pdf-encrypt` 的套件版本。可用 `cargo check --offline` 讓 Cargo 更新鎖定檔；快取不足時使用一般 `cargo check`。檢查差異，確保沒有順帶升級相依套件。
- **不要全專案取代舊版本字串。** `packaging/scoop/pdf-encrypt.json` 的 `0.1.0` 是 `render-manifest.sh` 使用的範本標記；Homebrew 範本與雜湊佔位值也不是本次套件升版的同步目標。發行資產由工作流程產生。
- `.github/workflows/release.yml` 含有自訂封裝與發布工作，不能為了升版直接用 `dist generate` 覆寫。若使用 cargo-dist 檢查規劃，先核對 `dist-workspace.toml` 指定的工具版本。
- 執行 `make check` 與 `make release`，再確認 `target/release/pdf-encrypt --version` 等於目標版本，並檢查頂層與 `encrypt --help` 可正常輸出。檢查須針對預定發布的內容；若工作目錄另有不納入發行的修改，在隔離工作樹驗證，確保受測程式碼與發行提交一致。`make release` 只建立最佳化執行檔，不會發布。
- 檢查失敗時先處理本次發行相關問題並重跑受影響的檢查；未解決前不推送會觸發發布的 ref。文件或版本紀錄只沿用專案既有慣例及使用者要求。

* * *

## 提交並觸發發行

1. 檢視完整差異，明確指定要暫存的檔案或區塊。提交前檢視 `git diff --cached`，確保只有本次授權內容。提交訊息遵守 Conventional Commits 1.0.0，使用 UTF-8 亂數暫存檔，subject 後保留空白行。例如先將 `release_version` 設為已確認的目標版本，再執行：

   ```sh
   commit_msg_file="$(mktemp -t codex-commit-message)"
   printf 'chore(release): bump version to %s\n\n同步套件與鎖定檔版本。\n' "$release_version" > "$commit_msg_file"
   git commit -F "$commit_msg_file"
   ```

   正文依實際發行內容調整。提交成功後刪除暫存檔；失敗時保留訊息並處理原因。

2. 記錄發行提交 SHA。現有 `Release` 工作流程會在推送 `main` 後，以 `Cargo.toml` 版本發布尚不存在的 Release。標準發行只推送該提交至 `main`，讓工作流程建立標籤與資產；同一版本不要再推送標籤或額外 dispatch，以免重複觸發。若流程已變更，依目前設定選擇單一觸發方式。
3. 推送前確認已授權發布該版本及變更範圍。使用者已明確要求本次發行時直接進行；只授權本機準備時，先提供版本、提交、檢查結果及推送會發布的內容，再取得缺少的授權。
4. 由 CI 處理 GitHub Release、安裝程式、封裝與 crates.io 發布。一般發行不另行執行 `gh release create` 或 `cargo publish`。crates.io 使用 `crates-io` environment 的 OIDC Trusted Publishing，由認證 action 取得短期 token，不使用長期 GitHub secret。認證失敗須處理 Trusted Publisher 設定，不能略過或據此聲稱 crate 已發布。首次啟用及設定值參閱 [Trusted Publishing 設定](../../../docs/trusted-publishing.md)。
5. npm 套件 `@willh/pdf-encrypt` 及 6 個平台套件由 `publish-npm` job 發布，版本取自 Release 標籤，`npm/` 目錄沒有需要同步的版本字串。該 job 使用 `npm` environment，優先使用 OIDC Trusted Publishing，`NPM_TOKEN` secret 為備援；registry 已有相同版本時略過。若要回報 npm 已發布，以 `npm view @willh/pdf-encrypt version` 查證。設定方式參閱 [npm 發布](../../../docs/npm-publishing.md)。

* * *

## 確認完成與處理重跑

- 透過 `gh run list` 找到本次提交 SHA 對應的 `CI` 與 `Release` 執行，追蹤至完成；可用 `gh run watch --exit-status`，失敗時讀取 `gh run view --log-failed`。辨識事件與提交，避免將舊執行的成功套用到本次發行。
- **Release 頁面出現不代表全部發行工作已完成。** GitHub Release 建立後還有 Linux、macOS、套件管理器資訊及 crates.io 工作；核對完整工作流程結果與目前設定要求的資產，不能只等 `host` 成功。
- 查驗 Release 的標籤實際指向本次發行提交，核對版本、資產及可用的 SHA-256 資訊。若要回報 crates.io 已發布，另以該工作日誌及 registry 查詢確認，不能只看 job 成功。
- 僅驗證 Trusted Publishing 設定時，可 dispatch `release.yml`，設定 `verify_trusted_publishing=true`、`republish_existing=false`，使用 `main` ref。此模式只驗證 OIDC 認證，不發布 crate、不重建或覆寫 Release 資產；成功不能當成版本已發布。
- 版本、標籤或 Release 已存在時，先判斷是完整發行、部分失敗，還是使用者要求重發。同一輪失敗先查明原因；重跑前確認既有執行已結束。僅在原因已修正且重跑仍屬本次授權範圍時進行，若相同原因再次失敗則停止並回報。
- 使用者要求重發既有資產時，使用現有 `workflow_dispatch` 的 `version` 與 `republish_existing=true`，並指定包含相同 `Cargo.toml` 版本及預期程式碼的 ref。該操作會覆寫同名資產，須在已授權範圍內使用；此模式略過 crates.io 發布。維持既有標籤，不移動或強制推送已發布的標籤。需要修改已發布程式碼時，改發新版本。
- 完成後以正體中文回報版本、提交 SHA、標籤與 Release 連結、檢查結果，以及 crates.io 的實際狀態。若只能完成本機準備、工作流程仍在執行或有工作失敗，明確標示已完成與尚未完成的部分。
