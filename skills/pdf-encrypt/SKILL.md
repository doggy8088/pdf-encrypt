---
name: pdf-encrypt
description: Read PDF document properties, encrypt, decrypt, re-encrypt, and apply PDF viewer permissions/metadata with the local Rust `pdf-encrypt` CLI. Use when a user wants to inspect PDF metadata, prefill document information, password-protect a PDF, remove PDF encryption when they have the password, create read-only PDF copies, set PDF permissions, change PDF encryption algorithms, or update PDF metadata while encrypting.
---

# pdf-encrypt

Use this skill to encrypt, decrypt, or re-encrypt PDF files with the `pdf-encrypt` command-line tool in this repository.

## What this tool does

`pdf-encrypt` uses `lopdf` directly to read and write PDFs. It does not shell out to other PDF utilities.

It can:

- Encrypt PDFs with `aes-256` by default.
- Read document properties with `info` / `properties`, including JSON output for metadata defaults.
- Decrypt encrypted PDFs when the correct user/open password or owner password is available.
- Re-encrypt an already encrypted PDF by opening it with the old password and writing a new encrypted output.
- Create PDFs that do not require an open password but restrict viewer permissions through an owner password.
- Configure viewer permissions such as print, copy, modify, annotate, fill forms, accessibility extraction, and page assembly.
- Apply `--read-only` / `-ro`, which means `--allow all --deny modify --deny assemble`.
- Update Document Info / XMP metadata during encryption.
- Encrypt in place with `--in-place` after writing a temporary file and replacing the input only after success.

PDF permissions are advisory restrictions that compliant PDF readers should honor. They are not an unbreakable access-control system. Confidentiality comes from a strong open password and encryption.

## Tool resolution is mandatory

This skill must run the official `pdf-encrypt` executable only. Do **not** use any other PDF encryption/decryption tool or OS PDF API as a fallback.

Forbidden fallbacks include, but are not limited to: `qpdf`, `gs`, Ghostscript, `mutool`, `pdftk`, `cpdf`, `pdfcpu`, ImageMagick `magick`/`convert`, macOS Preview, Quartz filters, PDFKit helper programs, Automator actions, AppleScript, browser print-to-PDF, online services, package-manager installs, or newly generated helper source code.

Before processing PDFs, resolve the executable in this exact order and stop as soon as one step succeeds:

1. Matching executable under this skill's `scripts/` directory.
2. Auto-download the matching uncompressed executable from the latest GitHub Release into this skill's `scripts/` directory.
3. `target/release/pdf-encrypt` in the current repository, only when the current working directory is a `pdf-encrypt` source checkout.
4. `pdf-encrypt` available on `PATH`.
5. Build from source with `cargo build --release`, only when the current working directory is a `pdf-encrypt` source checkout.

If all five steps fail, stop and report that the official `pdf-encrypt` executable could not be resolved. Do not search for or run alternative PDF tools.

### Skill directory and scripts directory

`SKILL_DIR` means the directory that contains this `SKILL.md` file. It is not necessarily the repository's `skills/pdf-encrypt` path after the skill is installed.

Always store downloaded executables in:

```text
<SKILL_DIR>/scripts/<asset-name>
```

For example, if the skill is installed at `/Users/will/.agents/skills/pdf-encrypt`, the macOS Apple Silicon executable must be downloaded to:

```text
/Users/will/.agents/skills/pdf-encrypt/scripts/pdf-encrypt-aarch64-apple-darwin
```

Create `<SKILL_DIR>/scripts/` if it does not exist.

### Platform asset mapping

Detect the current platform once, map it to exactly one release asset, then use only that asset name.

| Platform | CPU | Release asset |
| --- | --- | --- |
| macOS | Apple Silicon / `arm64` | `pdf-encrypt-aarch64-apple-darwin` |
| macOS | Intel / `x86_64` | `pdf-encrypt-x86_64-apple-darwin` |
| Linux | `x86_64` | `pdf-encrypt-x86_64-unknown-linux-gnu` |
| Linux | `aarch64` / `arm64` | `pdf-encrypt-aarch64-unknown-linux-gnu` |
| Windows | `AMD64` / `x86_64` | `pdf-encrypt-x86_64-pc-windows-msvc.exe` |
| Windows | `ARM64` / `aarch64` | `pdf-encrypt-aarch64-pc-windows-msvc.exe` |

Do not try similar names. Do not scan release archives. Do not infer a different target triple.

### Download rules

Only download the exact uncompressed executable asset listed above. Do **not** download `.zip`, `.tar.xz`, `.tar.gz`, `.msi`, `.pkg`, `.deb`, `.rpm`, installer scripts, Homebrew formulae, Scoop manifests, or any archive/package that requires extraction or installation.

Download from:

```text
https://github.com/doggy8088/pdf-encrypt/releases/latest/download/<asset-name>
```

Save the response body directly to `<SKILL_DIR>/scripts/<asset-name>`. On Unix, run `chmod +x <SKILL_DIR>/scripts/<asset-name>` before executing it.

If `<asset-name>.sha256` is available from the same release, download it to `<SKILL_DIR>/scripts/<asset-name>.sha256` and verify the executable. If checksum download is unavailable, continue only if the executable download succeeded and the file is non-empty.

Treat the download as failed if the HTTP status is not successful, the saved file is empty, or the downloaded file is HTML/XML/JSON error text instead of a binary executable.

### Exact resolver template

Follow this structure. Replace `SKILL_DIR` with the directory containing this `SKILL.md`.

```bash
set -euo pipefail

SKILL_DIR="/absolute/path/to/the/directory/containing/SKILL.md"
SCRIPTS_DIR="$SKILL_DIR/scripts"
mkdir -p "$SCRIPTS_DIR"

case "$(uname -s):$(uname -m)" in
  Darwin:arm64) ASSET="pdf-encrypt-aarch64-apple-darwin" ;;
  Darwin:x86_64) ASSET="pdf-encrypt-x86_64-apple-darwin" ;;
  Linux:x86_64) ASSET="pdf-encrypt-x86_64-unknown-linux-gnu" ;;
  Linux:aarch64|Linux:arm64) ASSET="pdf-encrypt-aarch64-unknown-linux-gnu" ;;
  MINGW*:x86_64|MSYS*:x86_64|CYGWIN*:x86_64) ASSET="pdf-encrypt-x86_64-pc-windows-msvc.exe" ;;
  MINGW*:aarch64|MINGW*:arm64|MSYS*:aarch64|MSYS*:arm64|CYGWIN*:aarch64|CYGWIN*:arm64) ASSET="pdf-encrypt-aarch64-pc-windows-msvc.exe" ;;
  *) echo "Unsupported platform for pdf-encrypt auto-download" >&2; exit 1 ;;
esac

TOOL="$SCRIPTS_DIR/$ASSET"

if [ ! -x "$TOOL" ]; then
  URL="https://github.com/doggy8088/pdf-encrypt/releases/latest/download/$ASSET"
  TMP="$TOOL.tmp"
  curl -fL "$URL" -o "$TMP"
  test -s "$TMP"
  case "$ASSET" in
    *.exe) ;;
    *) chmod +x "$TMP" ;;
  esac
  mv "$TMP" "$TOOL"
fi
```

After this resolver succeeds, run `"$TOOL"` for PDF encryption/decryption. Do not replace `"$TOOL"` with another PDF command, and do not run `find` to discover other executables.

## Build the tool

From the repository root:

```bash
cargo build --release
```

The release binary is created at:

```bash
target/release/pdf-encrypt
```

For quick local runs during development, `cargo run --` is also acceptable. Building is allowed only when no suitable skill-local, repository-local, PATH, or downloadable official release binary is available and the current working directory is a `pdf-encrypt` source checkout.

## Basic usage

All examples assume the mandatory resolver has already set `TOOL` to the official `pdf-encrypt` executable.

Encrypt a PDF and require a password to open it:

```bash
"$TOOL" encrypt \
  --input original.pdf \
  --output encrypted.pdf \
  --open-password "$PDF_OPEN_PASSWORD"
```

Decrypt a PDF when the password is known:

```bash
"$TOOL" decrypt \
  --input encrypted.pdf \
  --output decrypted.pdf \
  --password "$PDF_OPEN_PASSWORD"
```

Development-only equivalent when working inside a `pdf-encrypt` source checkout:

```bash
cargo run -- encrypt \
  --input original.pdf \
  --output encrypted.pdf \
  --open-password "$PDF_OPEN_PASSWORD"
```

## Commands and arguments

### `info` / `properties`

以唯讀方式查詢文件屬性，不會改寫原檔或建立解密副本。AI 代理在詢問文件資訊欄位前，先執行：

```bash
"$TOOL" info -i "$PDF_INPUT" --json
```

以 `document_info` 的八個欄位預填 `--doc-*` 值，再一次請使用者確認；只補充缺值、衝突、無法讀取或要修改的內容。使用者已指定的值優先於原檔屬性，不要求重新輸入已有值。密碼仍須由使用者提供。

| 參數 | 必要 | 預設 | 用途 |
| --- | --- | --- | --- |
| `--input`、`-i` | 是 | 無 | 輸入 PDF 路徑 |
| `--password`、`-p` | 否 | 空密碼 | 開啟密碼；AES-256 亦可使用擁有者密碼 |
| `--json` | 否 | 文字報表 | 輸出純 JSON，便於預填欄位 |

`document_info` 優先採用 Document Info，缺值再取 XMP。`null` 表示未取得值，`""` 表示空字串；預填前檢查 `info_status`、`xmp_status`、`conflicts` 與 `warnings`，不可將讀取失敗視為空白。來源狀態可為 `absent`、`read`、`partial`、`unreadable`。額外屬性保留在 `info_dictionary` 與 `xmp_properties`。

需要開啟密碼時先向使用者取得，再加上 `--password "$PDF_PASSWORD"`。目前 PDF 相依套件無法可靠地以舊版擁有者密碼讀取 AES-128／RC4；這類文件若開啟密碼非空，必須提供開啟密碼。讀取失敗時回報原因，不另用其他工具繞過。已解析的執行檔若尚不支援 `info`，須回報需要包含此功能的版本。

### `encrypt`

| Argument | Required | Default | Purpose |
| --- | --- | --- | --- |
| `--input`, `-i` | Yes | None | Source PDF path. |
| `--output`, `-o` | No | Adds `_encrypted` before the input extension | Destination PDF path. Conflicts with `--in-place`. |
| `--in-place` | No | `false` | Encrypt and replace the input file after successful temporary output creation. Conflicts with `--output`. |
| `--force`, `-f` | No | `false` | Skip overwrite confirmation prompts. |
| `--open-password`, `-p` | One of open/owner password is required | Empty if only owner password is set | Password required to open the output PDF. If owner password is omitted, this is also used as owner password. |
| `--owner-password` | One of open/owner password is required | Same as open password | Owner password. If set without open password, the output opens without a password but has restricted permissions. |
| `--input-open-password` | No | None | Password used to open an encrypted input before re-encrypting. |
| `--read-only`, `-ro` | No | `false` | Equivalent to allowing all permissions and denying `modify` and `assemble`. |
| `--algorithm` | No | `aes-256` | Encryption algorithm: `aes-256`, `aes-128`, `rc4-128`, or `rc4-40`. |
| `--allow` | No | All permissions | Repeatable permission allow-list. If used, only listed permissions are allowed. |
| `--deny` | No | None | Repeatable permission deny-list removed from the current permission set. |
| `--doc-title` | No | Existing/missing metadata | Set Document Title / XMP `dc:title`. |
| `--doc-author` | No | Existing/missing metadata | Set Author / XMP `dc:creator`. |
| `--doc-subject` | No | Existing/missing metadata | Set Subject. |
| `--doc-keywords` | No | Existing/missing metadata | Set keywords. |
| `--doc-description` | No | Existing/missing metadata | Set description. |
| `--doc-copyright-status` | No | Existing/missing metadata | Set copyright status, such as `Copyrighted` or `Public Domain`. |
| `--doc-copyright-notice` | No | Existing/missing metadata | Set copyright notice. |
| `--doc-copyright-info-url` | No | Existing/missing metadata | Set copyright info URL. |
| `--no-encrypt-metadata` | No | `false` | Leave PDF metadata stream unencrypted. |

### `decrypt`

| Argument | Required | Default | Purpose |
| --- | --- | --- | --- |
| `--input`, `-i` | Yes | None | Source encrypted PDF path. |
| `--output`, `-o` | No | Adds `_decrypted` before the input extension | Destination PDF path. |
| `--password`, `-p` | No | Empty string | User/open or owner password. Use `--password ""` for empty-password encryption. |

### Permission values

`--allow` and `--deny` accept these values:

- `all`
- `print`
- `high-quality-print`
- `modify`
- `copy`
- `annotate`
- `fill-forms`
- `accessibility`
- `assemble`

## Recommended workflow

1. Resolve the official `pdf-encrypt` executable using the mandatory resolver order above.
2. If no official executable can be resolved, stop with an error instead of trying unrelated PDF tools.
3. Preserve the original PDF unless the user explicitly asks for `--in-place`.
4. Ask the user for required passwords when they are not provided. Prefer passing passwords through environment variables in shell examples instead of embedding secrets directly in commands.
   When collecting document information, first read `"$TOOL" info -i "$PDF_INPUT" --json` and use the existing properties as defaults for a single confirmation.
5. For confidential PDFs, prefer `--open-password` with a strong password and keep the default `--algorithm aes-256`.
6. For read-only sharing without requiring an open password, use `--owner-password` with `--read-only` and optionally stricter `--deny print --deny high-quality-print --deny copy`.
7. Use `--input-open-password` when re-encrypting an existing encrypted PDF.
8. Use explicit output paths so results are easy to identify and accidental overwrites are avoided.
9. Use `--force` only when the user explicitly approves overwriting an existing file.
10. Review the CLI report after completion for operation, algorithm, input/output sizes, encryption state, page count, and object count.

## Safety and validation

Always preserve the original PDF unless in-place replacement is explicitly requested.

After encryption or decryption, verify that:

- The command exited successfully.
- The output PDF exists and has non-zero size.
- The report shows the expected `operation` and `output_encrypted` state.
- The password can open the encrypted output, or the decrypted output opens without encryption as expected.
- Important pages render correctly.

Useful size check:

```bash
ls -lh original.pdf encrypted.pdf
```

## Examples

Encrypt with an open password using the default `aes-256` algorithm:

```bash
PDF_OPEN_PASSWORD='change-me-to-a-strong-password'
"$TOOL" encrypt \
  --input report.pdf \
  --output report_locked.pdf \
  --open-password "$PDF_OPEN_PASSWORD"
```

Create a read-only PDF that opens without a password but requires an owner password to change permissions:

```bash
PDF_OWNER_PASSWORD='owner-secret'
"$TOOL" encrypt \
  --input report.pdf \
  --output report_readonly.pdf \
  --owner-password "$PDF_OWNER_PASSWORD" \
  --read-only
```

Create a stricter read-only PDF that also denies printing and copying:

```bash
"$TOOL" encrypt \
  --input report.pdf \
  --output report_view_only.pdf \
  --owner-password "$PDF_OWNER_PASSWORD" \
  --read-only \
  --deny print \
  --deny high-quality-print \
  --deny copy
```

Re-encrypt an encrypted PDF with a new password:

```bash
PDF_OLD_PASSWORD='old-secret'
PDF_NEW_PASSWORD='new-secret'
"$TOOL" encrypt \
  --input old_locked.pdf \
  --output new_locked.pdf \
  --input-open-password "$PDF_OLD_PASSWORD" \
  --open-password "$PDF_NEW_PASSWORD"
```

Decrypt a PDF:

```bash
"$TOOL" decrypt \
  --input report_locked.pdf \
  --output report_plain.pdf \
  --password "$PDF_OPEN_PASSWORD"
```

Encrypt and update metadata:

```bash
"$TOOL" encrypt \
  --input report.pdf \
  --output report_locked.pdf \
  --open-password "$PDF_OPEN_PASSWORD" \
  --doc-title "規格驅動開發實戰" \
  --doc-author "Will 保哥" \
  --doc-subject "AI 時代的軟體開發新典範" \
  --doc-keywords "AI, SDD, Rust" \
  --doc-description "課程講義" \
  --doc-copyright-status "Copyrighted" \
  --doc-copyright-notice "Copyright © Will 保哥"
```

Batch-encrypt PDFs into a separate folder:

```bash
PDF_OPEN_PASSWORD='change-me-to-a-strong-password'
mkdir -p encrypted
for pdf in *.pdf; do
  "$TOOL" encrypt \
    --input "$pdf" \
    --output "encrypted/${pdf%.pdf}-encrypted.pdf" \
    --open-password "$PDF_OPEN_PASSWORD"
done
```
