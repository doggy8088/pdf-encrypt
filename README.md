# pdf-encrypt：PDF 加密與解密工具

`pdf-encrypt` 是以 Rust 撰寫的命令列工具，用來加密或解密 PDF 文件。實作風格參考 `pdf-shrink`：CLI 負責解析參數與列印報表，核心 PDF 處理邏輯放在 library API，並使用 `lopdf` 直接讀寫 PDF，不依賴外部 PDF 工具。

## 功能

- 加密 PDF，預設使用 `aes-256`。
- 解密已加密 PDF，輸出不含 `/Encrypt` dictionary 的一般 PDF。
- 支援使用者密碼、擁有者密碼與 read-only password。
- 可設定 PDF viewer 權限，例如列印、複製、修改、註解、填表等。
- 可用 `--open-password` 先開啟已加密輸入檔，再重新加密成新密碼或新演算法。

> 注意：PDF 權限是 PDF reader 應遵守的限制，不等同於無法被破解的存取控制。真正保護內容機密性的是加密密碼本身。

## 安裝

### 前置需求

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

若是透過 `cargo install` 安裝，可用下列指令移除：

```bash
cargo uninstall pdf-encrypt
```

## 使用方式

### 加密

```bash
pdf-encrypt encrypt --input original.pdf --output encrypted.pdf --user-password secret
```

未指定 `--output` 時，會自動在原檔名後加上 `_encrypted`：

```bash
pdf-encrypt encrypt -i report.pdf -p secret
# 輸出 report_encrypted.pdf
```

建立 read-only password：

```bash
pdf-encrypt encrypt \
  -i report.pdf \
  -o report_readonly.pdf \
  --read-only-password viewer-secret \
  --owner-password owner-secret
```

`--read-only-password` 會建立受限的 user password，預設禁止修改內容、註解、填寫表單與重組頁面；`--owner-password` 則是保留完整管理權限的密碼，必須與 read-only password 不同。若想更接近只能觀看、不能列印/複製，可再加上：

```bash
--deny print --deny high-quality-print --deny copy
```

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

未指定 `--allow` 時，`--user-password` 預設允許全部權限，`--read-only-password` 預設使用 read-only 權限。若指定 `--allow`，則只允許列出的權限；也可以再用 `--deny` 從目前允許權限中移除。

```bash
pdf-encrypt encrypt \
  -i report.pdf \
  -o report_readonly.pdf \
  -p secret \
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
  --open-password old-secret \
  --user-password new-secret \
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
use pdf_encrypt::{decrypt_pdf, encrypt_pdf, encrypt_pdf_with_options, EncryptOptions};

fn main() -> anyhow::Result<()> {
    encrypt_pdf("input.pdf", "locked.pdf", "secret")?;

    let options = EncryptOptions::with_read_only_password("viewer-secret", "owner-secret");
    encrypt_pdf_with_options("input.pdf", "readonly.pdf", &options)?;

    decrypt_pdf("locked.pdf", "plain.pdf", "secret")?;
    Ok(())
}
```

## 開發檢查

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```
