# macOS pkg 簽署與公證

Release 工作流程的 `macos-pkg` job 會簽署並公證 `pdf-encrypt-<target>.pkg`：

1. 以 **Developer ID Application** 簽署 pkg 內的 `pdf-encrypt` 執行檔（hardened runtime、安全時間戳）。
2. 以 **Developer ID Installer** 簽署 `.pkg`。
3. 以 App Store Connect API key 送 Apple 公證並等待結果；失敗時輸出 notarytool log 並中止。
4. 將公證票證 staple 到 `.pkg`，並以 `stapler validate`、`spctl --assess --type install` 驗證。

腳本為 `packaging/macos/build-pkg.sh` 與 `packaging/macos/notarize-pkg.sh`。憑證只匯入 job 內的暫時 keychain，結束時刪除。缺少任何設定時 job 直接失敗，不會發布未簽署的 pkg。

* * *

## GitHub 設定

設定位於 `macos-signing` environment，部署來源限定 `main` 分支與 `v*` 標籤。

| 類型 | 名稱 | 內容 |
| --- | --- | --- |
| Secret | `MACOS_APP_P12_BASE64` | Developer ID Application `.p12` 的 base64 |
| Secret | `MACOS_APP_P12_PASSWORD` | 上述 `.p12` 的密碼 |
| Secret | `MACOS_INSTALLER_P12_BASE64` | Developer ID Installer `.p12` 的 base64（含 Developer ID G2 中繼憑證） |
| Secret | `MACOS_INSTALLER_P12_PASSWORD` | 上述 `.p12` 的密碼 |
| Secret | `APPLE_API_KEY_P8` | App Store Connect API key（`.p8` 檔內容） |
| Variable | `MACOS_APP_IDENTITY` | `Developer ID Application: Duotify Inc. (XKF69LC6US)` |
| Variable | `MACOS_INSTALLER_IDENTITY` | `Developer ID Installer: Duotify Inc. (XKF69LC6US)` |
| Variable | `APPLE_API_KEY_ID` | `3X5KRBCA78` |
| Variable | `APPLE_API_ISSUER_ID` | App Store Connect Issuer ID |

更新 secret 時，從檔案以管線傳入，避免值出現在畫面或 shell 歷史：

```sh
base64 -i "Developer ID Installer- Duotify Inc. (XKF69LC6US).p12" | tr -d '\n' \
  | gh secret set MACOS_INSTALLER_P12_BASE64 --repo doggy8088/pdf-encrypt --env macos-signing
gh secret set MACOS_APP_P12_PASSWORD --repo doggy8088/pdf-encrypt --env macos-signing   # 互動輸入
```

* * *

## 驗證設定

`verify_trusted_publishing=true` 模式會在不上傳任何資產的情況下，完整執行一次簽署、公證與 staple：

```sh
gh workflow run release.yml --repo doggy8088/pdf-encrypt --ref main \
  -f verify_trusted_publishing=true -f republish_existing=false
```

本機驗證（需要兩個 identity 在登入 keychain）：

```sh
MACOS_APP_IDENTITY="Developer ID Application: Duotify Inc. (XKF69LC6US)" \
MACOS_INSTALLER_IDENTITY="Developer ID Installer: Duotify Inc. (XKF69LC6US)" \
  packaging/macos/build-pkg.sh target/release/pdf-encrypt 0.0.0 /tmp/pdf-encrypt.pkg
APPLE_API_KEY_PATH=AuthKey_3X5KRBCA78.p8 APPLE_API_KEY_ID=3X5KRBCA78 APPLE_API_ISSUER_ID=<issuer> \
  packaging/macos/notarize-pkg.sh /tmp/pdf-encrypt.pkg
```

* * *

## 到期與輪替

| 憑證 | 到期日 |
| --- | --- |
| Developer ID Installer: Duotify Inc. (XKF69LC6US) | 2027-11-13 |
| Developer ID Application: Duotify Inc. (XKF69LC6US) | 2031-05-26 |

Installer 憑證到期前，在 Apple Developer **Certificates** 頁以新的 CSR 建立新的 Developer ID Installer 憑證（Apple 不接受重複使用的 CSR），匯出 `.p12` 後更新 `MACOS_INSTALLER_P12_BASE64` 與 `MACOS_INSTALLER_P12_PASSWORD`，再執行上方的驗證模式。已公證並 staple 的 pkg 在憑證到期後仍可安裝。
