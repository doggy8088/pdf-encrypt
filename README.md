# pdf-encrypt：PDF 加密與解密工具

`pdf-encrypt` 是以 Rust 撰寫的命令列工具，用來加密、解密及讀寫 PDF 文件屬性。實作風格參考 `pdf-shrink`：CLI 負責解析參數與列印報表，核心 PDF 處理邏輯放在 library API，並使用 `lopdf` 直接讀寫 PDF，不依賴外部 PDF 工具。

## 功能

- 加密 PDF，預設使用 `aes-256`。
- 解密已加密 PDF，輸出不含 `/Encrypt` dictionary 的一般 PDF。
- 可用 `info` 唯讀查詢 PDF 文件屬性，並用 `--json` 供 AI 代理預填文件資訊欄位。
- 可用 `edit` 獨立編輯文件屬性，保留未指定的欄位、自訂 XMP 與既有加密設定。
- 支援開啟文件密碼 (`--open-password`) 與擁有者密碼 (`--owner-password`)。
- 可設定 PDF viewer 權限，例如列印、複製、修改、註解、填表等。
- 可用 `--input-open-password` 先開啟已加密輸入檔，再重新加密成新密碼或新演算法。
- 可用 `-ro` / `--read-only` 將開啟文件後的權限設定為 allow all 並 deny modify 與 assemble。
- 加密前可用 `--doc-*` 參數更新 PDF Document Info / XMP metadata。
- 可用 `--in-place` 直接加密並覆蓋原輸入檔案，並可用 `--force` 略過覆蓋確認。

網站：<https://pdf-encrypt.gh.miniasp.com/>（安裝方式、指令產生器與常見問題）

> 注意：PDF 權限是 PDF reader 應遵守的限制，不等同於無法被破解的存取控制。真正保護內容機密性的是加密密碼本身。

## 安裝

### 安裝指令稿（macOS、Linux）

不需要 Rust 開發環境。指令稿會從 GitHub Releases 下載對應平台的執行檔，驗證 SHA-256 後安裝到 `~/.local/bin/pdf-encrypt`：

```bash
curl -fsSL https://raw.githubusercontent.com/doggy8088/pdf-encrypt/main/install.sh | sh
```

指定版本或安裝目錄：

```bash
curl -fsSL https://raw.githubusercontent.com/doggy8088/pdf-encrypt/main/install.sh | sh -s -- --version 0.3.0 --to ~/bin
```

也可以用環境變數 `PDF_ENCRYPT_VERSION` 與 `PDF_ENCRYPT_INSTALL_DIR` 設定。若安裝目錄不在 `PATH`，指令稿會提示要加入 shell 設定檔的內容。Linux 使用靜態連結的 musl 版本，glibc 與 musl 發行版皆可執行。

### 安裝指令稿（Windows）

在 PowerShell 執行，會安裝到 `%LOCALAPPDATA%\Programs\pdf-encrypt\bin`，並將該目錄加入使用者 `PATH`：

```powershell
irm https://raw.githubusercontent.com/doggy8088/pdf-encrypt/main/install.ps1 | iex
```

指定版本或安裝目錄：

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/doggy8088/pdf-encrypt/main/install.ps1))) -Version 0.3.0 -InstallDir C:\Tools\pdf-encrypt
```

也可以用環境變數 `PDF_ENCRYPT_VERSION`、`PDF_ENCRYPT_INSTALL_DIR` 設定；設定 `PDF_ENCRYPT_NO_MODIFY_PATH=1` 時不修改 `PATH`。

### npm

已安裝 Node.js 18 以上版本時，可透過 npm 安裝。npm 只會下載符合目前作業系統與 CPU 的預先編譯執行檔，安裝時不執行 postinstall 指令稿：

```bash
npm install -g @willh/pdf-encrypt
```

不安裝直接執行：

```bash
npx @willh/pdf-encrypt --help
```

### 從原始碼安裝的前置需求

請先安裝 [Rust 開發工具鏈](https://www.rust-lang.org/tools/install)，確認終端機可以使用 `cargo`：

```bash
cargo --version
```

### 從本機原始碼安裝

在本專案根目錄執行：

```bash
cargo install --path pdf-encrypt --locked
```

或進入 `pdf-encrypt/` 目錄後安裝：

```bash
cd pdf-encrypt
cargo install --path . --locked
```

安裝完成後，執行檔會放在 Cargo 的 bin 目錄，通常是 `~/.cargo/bin`。若系統找不到 `pdf-encrypt` 指令，請確認該目錄已加入 `PATH`。

驗證安裝：

```bash
pdf-encrypt --version
pdf-encrypt --help
```

### 建置但不安裝

如果只想產生 release binary 而不安裝到 PATH：

```bash
cd pdf-encrypt
cargo build --release --locked
```

建置完成後，可在 `target/release/pdf-encrypt` 找到執行檔。

### 移除安裝

依安裝方式移除：

```bash
# cargo install
cargo uninstall pdf-encrypt

# npm
npm uninstall -g @willh/pdf-encrypt

# install.sh（預設位置）
rm ~/.local/bin/pdf-encrypt
```

透過 `install.ps1` 安裝時，刪除 `%LOCALAPPDATA%\Programs\pdf-encrypt` 目錄，並從使用者 `PATH` 移除 `%LOCALAPPDATA%\Programs\pdf-encrypt\bin`。

## 使用方式

### 讀取文件屬性

查詢本工具或其他工具產生的 PDF，不會改寫原檔或產生解密副本：

```bash
pdf-encrypt info -i report.pdf
pdf-encrypt info -i report.pdf --json
```

`properties` 是 `info` 的別名。需要開啟密碼的輸入檔可用 `--password`，密碼值由使用者提供：

```bash
pdf-encrypt info -i encrypted.pdf --password "$PDF_PASSWORD" --json
```

JSON 輸出包含以下欄位：

| 欄位 | 說明 |
| --- | --- |
| `pdf_version`、`pages`、`input_encrypted` | PDF 版本、頁數及原檔是否加密 |
| `document_info` | 對應八個 `--doc-*` 參數的預填值；優先使用 Document Info，缺值再取 XMP |
| `info_dictionary` | 可讀的標準及自訂 Document Info 欄位，包含 Creator、Producer、CreationDate、ModDate 等 |
| `xmp_properties` | 以 `{namespace URI}local-name` 為鍵的 XMP 屬性；陣列保留各項值，複合值保留 XML |
| `info_status`、`xmp_status` | 來源狀態：`absent` 不存在、`read` 已讀取、`partial` 部分讀取、`unreadable` 無法讀取 |
| `conflicts` | 同一欄位在 Document Info 與 XMP 中的不同值，包含兩個來源供使用者確認 |
| `warnings` | 欄位解碼、XMP 解析及無法轉成文字預設值等問題 |

`null` 表示未取得值，`""` 表示原值為空字串。讀取失敗時，仍會保留其他可讀的屬性，並透過來源狀態與警告說明原因。無法開啟或解析 PDF 時，指令會回傳非零退出碼，錯誤訊息寫入 stderr。

AI 代理應先執行 `info --json`，用 `document_info` 預填欄位並一次請使用者確認，只補充缺值、衝突、無法讀取或需要修改的值。使用者明確指定的值優先於原檔屬性。預填前須一併檢查 `conflicts`、來源狀態及 `warnings`，不能把解析失敗視為空白。

支援的加密方式沿用 `lopdf`。AES-256 可使用開啟密碼或擁有者密碼；AES-128／RC4 文件在開啟密碼非空時，須使用開啟密碼查詢，因為目前相依套件的舊版擁有者密碼解密可能產生錯誤值，`info` 會明確拒絕此情況。免開啟密碼的 PDF 可直接查詢。損毀的 PDF、不支援的加密方式或密碼編碼仍可能無法讀取。

* * *

### 編輯文件屬性

`edit` 沿用加密指令的八個 `--doc-*` 參數，只修改指定欄位，同步更新 Document Info 與 XMP。未指定的欄位及自訂屬性會保留；至少須指定一個欄位。

```bash
pdf-encrypt edit -i report.pdf --doc-title "新標題" --doc-author "作者"
pdf-encrypt edit -i report.pdf -o updated.pdf --doc-keywords ""
```

未指定 `-o` 時，預設另存為 `report_edited.pdf`。空字串代表清空值，省略參數代表保留原值。編輯描述時，會保留原主旨；若來源 XMP 以 `dc:description` 同時表示主旨及描述，會另存原主旨至自訂 XMP Subject。

加密 PDF 使用既有密碼，不必重新指定演算法或權限。輸出保留原有加密方式、開啟密碼、擁有者密碼、權限與 metadata 加密設定；未加密的輸入仍保持未加密。

```bash
pdf-encrypt edit -i encrypted.pdf -p "$PDF_PASSWORD" --doc-description "新描述"
```

AES-256 可使用開啟密碼或擁有者密碼；AES-128／RC4 若有非空開啟密碼，須使用開啟密碼。免開啟密碼的 PDF 可省略 `-p`。密碼驗證失敗會停止處理。

原地修改須明確使用 `--in-place`；不能用 `-o` 指向輸入檔。覆蓋既有輸出或原地修改會詢問確認，已取得覆蓋授權時可加 `--force`：

```bash
pdf-encrypt edit -i report.pdf --in-place --force --doc-title "新標題"
```

寫入會先儲存至輸出目錄的暫存檔，成功後才取代目標。既有 Info 或 XMP 格式錯誤、密碼錯誤或儲存失敗時，指令回傳非零退出碼並保留原檔與既有輸出。XMP 保留原有 UTF-8／UTF-16 編碼與未修改的 XML 屬性。

AI 代理應先用 `info --json` 讀取原值。使用者已指定的修改直接採用；尚不明確的修改連同原值一次請使用者確認，未修改欄位不需重新輸入。

* * *

### 加密

```bash
pdf-encrypt encrypt --input original.pdf --output encrypted.pdf --open-password secret
```

未指定 `--output` 時，會自動在原檔名後加上 `_encrypted`：

```bash
pdf-encrypt encrypt -i report.pdf -p secret
# 輸出 report_encrypted.pdf
```

若要直接覆蓋原輸入檔案，而不是另存新檔，可使用 `--in-place`。此模式會先在同一個目錄寫入暫存檔，成功後才取代原檔，且不可與 `--output` 同時使用：

```bash
pdf-encrypt encrypt \
  -i report.pdf \
  --owner-password owner-secret \
  --read-only \
  --in-place
```

當輸出檔案已存在，或使用 `--in-place` 會覆蓋原檔時，CLI 會先提示確認。若已確認要覆蓋，可加上 `--force` 略過提示：

```bash
pdf-encrypt encrypt \
  -i report.pdf \
  --owner-password owner-secret \
  --read-only \
  --in-place \
  --force
```

`--open-password` 是加密後「開啟文件」所需的密碼。未指定 `--owner-password` 時，擁有者密碼會自動使用相同密碼；若只指定 `--owner-password` 而沒有指定 `--open-password`，則輸出 PDF 不需要開啟密碼。若要保留一組不同的完整管理權限密碼，可另外指定：

```bash
pdf-encrypt encrypt \
  -i report.pdf \
  -o report_locked.pdf \
  --open-password viewer-secret \
  --owner-password owner-secret
```

建立 read-only 權限且不要求開啟密碼：

```bash
pdf-encrypt encrypt \
  -i report.pdf \
  -o report_readonly.pdf \
  --owner-password owner-secret \
  --read-only
```

若也要讓文件開啟時需要密碼，可同時指定 `--open-password`：

```bash
pdf-encrypt encrypt \
  -i report.pdf \
  -o report_readonly.pdf \
  --open-password viewer-secret \
  --owner-password owner-secret \
  --read-only
```

`--read-only` 也可寫成 `-ro`，其權限語意等同於 `--allow all --deny modify --deny assemble`。若想更接近只能觀看、不能列印/複製，可再加上：

```bash
--deny print --deny high-quality-print --deny copy
```

加密前更新文件資訊：

```bash
pdf-encrypt encrypt \
  -i report.pdf \
  -o report_locked.pdf \
  --open-password secret \
  --doc-title "規格驅動開發實戰" \
  --doc-author "Will 保哥" \
  --doc-subject "AI 時代的軟體開發新典範" \
  --doc-keywords "AI, SDD, Rust" \
  --doc-description "課程講義" \
  --doc-copyright-status "Copyrighted" \
  --doc-copyright-notice "Copyright © Will 保哥" \
  --doc-copyright-info-url "https://example.com/copyright"
```

`encrypt` 與 `edit` 共用的文件資訊參數：

| 參數 | 寫入資訊 |
| --- | --- |
| `--doc-title` | Document Title / XMP `dc:title` |
| `--doc-author` | Author / XMP `dc:creator` |
| `--doc-subject` | Subject / XMP 自訂 subject 欄位 |
| `--doc-keywords` | Keywords / XMP `pdf:Keywords` 與 `dc:subject` |
| `--doc-description` | Description / XMP `dc:description` |
| `--doc-copyright-status` | Copyright status；常見值可用 `Copyrighted` 或 `Public Domain` |
| `--doc-copyright-notice` | Copyright Notice / XMP `dc:rights` |
| `--doc-copyright-info-url` | Copyright info URL / XMP `xmpRights:WebStatement` |

指定演算法：

```bash
pdf-encrypt encrypt -i report.pdf -o report_locked.pdf -p secret --algorithm aes-256
```

可用演算法：

| 值 | 說明 |
| --- | --- |
| `aes-256` | 預設；PDF 2.0 AES-256 標準安全處理器。 |
| `aes-128` | PDF 1.5 AES-128 crypt filter，適合需要較舊 PDF reader 相容性的情境。 |
| `rc4-128` | PDF 1.4 RC4 128-bit；相容性用途。 |
| `rc4-40` | PDF 1.4 RC4 40-bit；舊系統相容性用途，不建議用於新文件。 |

### 權限

未指定 `--allow` 時，預設允許全部權限；加入 `--read-only` 時會從目前權限中移除 `modify` 與 `assemble`。若指定 `--allow`，則只允許列出的權限；也可以再用 `--deny` 從目前允許權限中移除。

```bash
pdf-encrypt encrypt \
  -i report.pdf \
  -o report_readonly.pdf \
  --open-password secret \
  --allow print \
  --allow high-quality-print \
  --allow accessibility
```

可用權限值：

| 值 | 說明 |
| --- | --- |
| `all` | 全部權限。 |
| `print` | 允許列印。 |
| `high-quality-print` | 允許高品質列印。 |
| `modify` | 允許修改內容。 |
| `copy` | 允許複製或擷取內容。 |
| `annotate` | 允許新增或修改註解。 |
| `fill-forms` | 允許填寫表單。 |
| `accessibility` | 允許輔助使用需求的內容擷取。 |
| `assemble` | 允許組合文件頁面或大綱。 |

### 重新加密既有加密 PDF

```bash
pdf-encrypt encrypt \
  -i old_locked.pdf \
  -o new_locked.pdf \
  --input-open-password old-secret \
  --open-password new-secret \
  --algorithm aes-256
```

### 解密

```bash
pdf-encrypt decrypt --input encrypted.pdf --output decrypted.pdf --password secret
```

未指定 `--output` 時，會自動在原檔名後加上 `_decrypted`：

```bash
pdf-encrypt decrypt -i report_encrypted.pdf -p secret
# 輸出 report_encrypted_decrypted.pdf
```

如果 PDF 使用空密碼加密，可省略 `--password` 或使用 `--password ""`。

## Library API 範例

```rust
use pdf_encrypt::{
    decrypt_pdf, edit_pdf_properties, encrypt_pdf, encrypt_pdf_with_options,
    read_only_permissions, DocumentInfo, EditOptions, EncryptOptions,
};

fn main() -> anyhow::Result<()> {
    encrypt_pdf("input.pdf", "locked.pdf", "secret")?;

    let mut options = EncryptOptions::with_open_password("viewer-secret");
    options.owner_password = "owner-secret".to_string();
    options.permissions = read_only_permissions();
    options.document_info.title = Some("規格驅動開發實戰".to_string());
    options.document_info.author = Some("Will 保哥".to_string());
    encrypt_pdf_with_options("input.pdf", "readonly.pdf", &options)?;

    decrypt_pdf("locked.pdf", "plain.pdf", "secret")?;

    let edit_options = EditOptions {
        document_info: DocumentInfo {
            title: Some("新標題".to_string()),
            ..DocumentInfo::default()
        },
        ..EditOptions::default()
    };
    edit_pdf_properties("input.pdf", "updated.pdf", &edit_options)?;
    Ok(())
}
```

## 開發檢查

crates.io 發布使用 OIDC Trusted Publishing；首次設定與認證驗證步驟見 [Trusted Publishing 設定](docs/trusted-publishing.md)。

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

## 網站

`public/` 是 <https://pdf-encrypt.gh.miniasp.com/> 的靜態網站，沒有建置步驟。推送到 `main` 且 `public/` 有變更時，`deploy-pages.yml` 會部署到 GitHub Pages。

- `public/index.html`：單頁網站，含安裝方式、情境範例、指令產生器與常見問題。
- `public/js/command-builder.js`：產生指令的純邏輯，不存取 DOM；`public/js/app.js` 負責頁面互動。
- `public/img/`：以 AI 生成的插圖與物件圖片。

本機預覽（網站使用根目錄絕對路徑，請以 `public/` 為根目錄啟動）：

```bash
python3 -m http.server 8000 --directory public
```

測試指令產生器；設定 `PDF_ENCRYPT_BIN` 時，會實際執行產生的 bash 指令：

```bash
cargo build --release --locked
PDF_ENCRYPT_BIN="$PWD/target/release/pdf-encrypt" node --test tests/site/*.test.mjs
```

指令產生器只在瀏覽器中組出文字，不會上傳檔案或儲存密碼；密碼預設以執行時輸入的環境變數傳遞。`Read-Host -MaskInput` 需要 PowerShell 7.1 以上。
