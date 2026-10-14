# npm 發布

npm 套件 `@willh/pdf-encrypt` 由 Release 工作流程的 `publish-npm` job 發布。套件不包含原始碼，只重新封裝 GitHub Release 已建置的執行檔。

* * *

## 套件結構

| 套件 | 內容 |
| --- | --- |
| `@willh/pdf-encrypt` | Node.js 啟動器 `bin/pdf-encrypt.js`，以 `optionalDependencies` 指向下列平台套件 |
| `@willh/pdf-encrypt-darwin-arm64` | `pdf-encrypt-aarch64-apple-darwin` |
| `@willh/pdf-encrypt-darwin-x64` | `pdf-encrypt-x86_64-apple-darwin` |
| `@willh/pdf-encrypt-linux-arm64` | `pdf-encrypt-aarch64-unknown-linux-musl` |
| `@willh/pdf-encrypt-linux-x64` | `pdf-encrypt-x86_64-unknown-linux-musl` |
| `@willh/pdf-encrypt-win32-arm64` | `pdf-encrypt-aarch64-pc-windows-msvc.exe` |
| `@willh/pdf-encrypt-win32-x64` | `pdf-encrypt-x86_64-pc-windows-msvc.exe` |

npm 依平台套件的 `os` 與 `cpu` 欄位，只安裝符合目前機器的套件。安裝過程不執行 postinstall，也不從 GitHub 下載檔案。

`package.json` 由 `npm/build-packages.mjs` 產生，版本取自 Release 標籤。升版時不需要修改 `npm/` 目錄。

* * *

## 首次設定

npm 只能替已存在的套件設定 Trusted Publisher，因此 7 個套件的第一個版本須以 token 發布。

1. 在 npmjs.com 確認可發布 `@willh` scope。
2. 建立 granular access token：權限 **Read and write**、範圍 **All packages**（套件尚不存在時無法限定）、勾選 **Bypass 2FA**，有效期限設短。
3. 建立 `publish-npm` job 使用的 `npm` environment，並將 token 存成該 environment 的 secret：

   ```sh
   gh api -X PUT repos/doggy8088/pdf-encrypt/environments/npm
   gh secret set NPM_TOKEN --repo doggy8088/pdf-encrypt --env npm
   ```

   可在 **Settings → Environments → npm** 將部署來源限定為 `main` 分支及 `v*` 標籤，與 `crates-io` environment 一致。

4. 發布第一個版本。若目前版本的 GitHub Release 已存在，可在本機用 token 補發：

   ```sh
   version=0.3.0
   gh release download "v$version" --repo doggy8088/pdf-encrypt --dir target/npm-binaries \
     --pattern 'pdf-encrypt-*-apple-darwin' \
     --pattern 'pdf-encrypt-*-unknown-linux-musl' \
     --pattern 'pdf-encrypt-*-pc-windows-msvc.exe'
   node npm/build-packages.mjs "$version" target/npm-binaries target/npm > target/npm-packages.txt
   npm login
   xargs bash npm/publish-packages.sh < target/npm-packages.txt
   ```

   或者直接等下一次發行，由 `publish-npm` job 使用 `NPM_TOKEN` 發布。

5. 7 個套件都存在後，逐一在 npmjs.com 套件頁的 **Settings → Trusted Publisher** 選擇 GitHub Actions 並填入：

   | 欄位 | 值 |
   | --- | --- |
   | Organization or user | `doggy8088` |
   | Repository | `pdf-encrypt` |
   | Workflow filename | `release.yml` |
   | Environment name | `npm` |

6. 刪除 `NPM_TOKEN` secret 並撤銷 token。之後的版本以 OIDC 發布並自動附上 provenance。可再於各套件的 **Settings → Publishing access** 選擇 **Require two-factor authentication and disallow tokens**。

* * *

## 驗證設定

`verify_trusted_publishing=true` 模式會同時驗證 crates.io 與 npm。npm 部分由 `npm/verify-trusted-publishing.mjs` 對 7 個套件逐一交換 OIDC token，不發布任何版本：

```sh
gh workflow run release.yml --repo doggy8088/pdf-encrypt --ref main \
  -f verify_trusted_publishing=true -f republish_existing=false
```

認證成功只代表設定可用。npm 顯示的 **Pending validation** 狀態須等第一次實際由 CI 發布後才會解除。

* * *

## 發布行為

- `publish-npm` 在 GitHub Release 建立後執行，先發布 6 個平台套件，最後發布主套件。
- registry 已有相同版本時略過該套件，因此重跑 job 或 `republish_existing=true` 不會失敗。npm 版本無法覆寫；需要修正時須發行新版本。
- 版本含有 `-`（例如 `1.0.0-rc.1`）時使用 `next` dist-tag，不影響 `latest`。
- Trusted Publisher 與 `NPM_TOKEN` 都沒有設定時，job 會在 `npm publish` 驗證失敗。GitHub Release 不受影響。

* * *

## 本機驗證

```sh
node npm/build-packages.mjs 0.3.0 target/npm-binaries target/npm
cd target/npm/pdf-encrypt-darwin-arm64 && npm pack && cd -
cd target/npm/pdf-encrypt && npm pack && cd -
mkdir -p target/npm-test && cd target/npm-test && npm init -y >/dev/null
npm install ../npm/pdf-encrypt-darwin-arm64/*.tgz ../npm/pdf-encrypt/*.tgz
npx pdf-encrypt --version
```

設定 `PDF_ENCRYPT_BINARY` 環境變數可讓啟動器改用指定的執行檔，方便除錯。
