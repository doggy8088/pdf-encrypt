---
name: pdf-encrypt
description: Read or edit PDF document properties, encrypt, decrypt, re-encrypt, and apply viewer permissions with the official `pdf-encrypt` CLI. Use whenever the user asks to inspect or update PDF metadata/properties, password-protect PDFs, remove PDF encryption, create read-only PDFs, or change PDF encryption settings.
---

# pdf-encrypt

當使用者要求讀取或修改 PDF 文件屬性（標題、作者、主旨、關鍵字、描述、著作權資訊等）、為 PDF 加密、解密、重新加密，或設定 PDF 檢視權限（唯讀、禁止列印、禁止複製等）時，使用本技能。

## 1. 僅限使用官方 `pdf-encrypt` 執行檔

必須且只能使用 `pdf-encrypt` 執行檔處理 PDF，**嚴禁**改用任何其他工具、系統 API 或自行撰寫腳本作為替代方案（禁止項目包含但不限於：`qpdf`、`gs` / Ghostscript、`mutool`、`pdftk`、`cpdf`、`pdfcpu`、ImageMagick、macOS Preview / Quartz / PDFKit、Python `pypdf` / `pikepdf` / `pymupdf` 等）。

依下列順序解析 `pdf-encrypt` 執行檔，一旦找到即可停止：

1. 系統 `PATH` 中的 `pdf-encrypt`（或 `~/.local/bin/pdf-encrypt`、`~/.cargo/bin/pdf-encrypt`）。
2. 若目前工作目錄為 `pdf-encrypt` 原始碼儲存庫，可使用 `./target/release/pdf-encrypt`。
3. 若尚未安裝，使用官方安裝方式取得執行檔（或透過 `npx -y @willh/pdf-encrypt` 執行）：
   - **macOS / Linux**：
     ```bash
     curl -fsSL https://raw.githubusercontent.com/doggy8088/pdf-encrypt/main/install.sh | sh
     ```
     （預設安裝至 `~/.local/bin/pdf-encrypt`）
   - **Windows (PowerShell)**：
     ```powershell
     irm https://raw.githubusercontent.com/doggy8088/pdf-encrypt/main/install.ps1 | iex
     ```
   - **Node.js / npm**：
     ```bash
     npm install -g @willh/pdf-encrypt
     ```

若無法取得或執行官方 `pdf-encrypt`，請直接停止並向使用者回報原因，切勿嘗試其他 PDF 工具。

## 2. 強制先讀取 `--help` 取得完整 Context 與規範

`pdf-encrypt` 已將所有 **AI Agent 執行與互動規範（`AI AGENT GUIDELINES`）**、預設加密流程、欄位預填規則、密碼語意與權限計算順序完整內建於 `--help` 訊息中。

在對使用者的 PDF 進行任何操作或向使用者詢問欄位前，**必須先執行以下指令讀取最新規範**：

1. 查看總覽與 AI 代理核心規範：
   ```bash
   pdf-encrypt --help
   ```
2. 根據任務需求，查看對應子命令的詳細規範與參數說明：
   ```bash
   pdf-encrypt info --help
   pdf-encrypt edit --help
   pdf-encrypt encrypt --help
   pdf-encrypt decrypt --help
   ```

請嚴格遵循 `--help` 輸出中的 `AI AGENT GUIDELINES`（包含密碼詢問原則、`info --json` 預填與一次確認流程、保留原檔與 `--force` 非互動覆蓋規則，以及執行後報表驗證）。
