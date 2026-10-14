# crates.io Trusted Publishing

本專案透過 GitHub Actions OIDC 取得短期發布 token，不在 GitHub Secrets 儲存長期 `CARGO_REGISTRY_TOKEN`。

* * *

## 首次設定

crates.io 要求套件已存在，才能設定 Trusted Publishing。首次發布須由套件維護者使用 API token；請將權限限制為 `publish-new`、套件範圍限制為 `pdf-encrypt`，完成後撤銷該 token。token 不應寫入提交紀錄、文件或對話。

在套件的 **Settings → Trusted Publishing** 新增 GitHub 設定：

| 欄位 | 值 |
| --- | --- |
| Repository owner | `doggy8088` |
| Repository name | `pdf-encrypt` |
| Workflow filename | `release.yml` |
| Environment | `crates-io` |

GitHub 儲存庫需有 `crates-io` environment，部署來源限定 `main` 分支及 `v*` 標籤。工作流程的 `publish-crates-io` job 已指定此 environment，以及 `contents: read`、`id-token: write` 權限。

官方認證 action 固定於已查驗的 `v1.0.5` commit，取得的 token 經環境變數提供給 `cargo publish --locked`，job 結束時由 action 撤銷。認證失敗會使 job 失敗，不會當成成功發布。

* * *

## 驗證設定

從 `main` 執行以下指令，驗證 OIDC 認證：

```sh
gh workflow run release.yml \
  --repo doggy8088/pdf-encrypt \
  --ref main \
  -f verify_trusted_publishing=true \
  -f republish_existing=false
```

此模式會略過平台建置、Release 上傳與 `cargo publish`，只驗證 Trusted Publishing 是否能取得短期 token。認證成功代表設定可用，不代表套件版本已發布。

正常發行沿用 `bump-and-release`：提升版本並推送 `main`，讓 Release 工作流程完成 GitHub Release 及 crates.io 發布。若重建既有 Release 資產，`republish_existing=true` 模式仍略過 crates.io，避免重複上傳已存在版本。

* * *

## 維護

- 儲存庫擁有者、名稱、工作流程檔名或 environment 變更時，同步修改 crates.io 的 Trusted Publisher。
- 可以在 crates.io 啟用 **Trusted Publishing Only**，讓後續版本只接受可信任工作流程發布。
- 更新認證 action 時，先核對官方 release 與 commit，再更新固定 SHA。
- crates.io 封裝排除 `test-output/**`，避免將大型測試 PDF 與渲染圖片上傳至 registry。

官方參考：[Trusted Publishing](https://crates.io/docs/trusted-publishing)、[認證 action](https://github.com/rust-lang/crates-io-auth-action)。
