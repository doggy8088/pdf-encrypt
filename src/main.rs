//! Command line interface for the `pdf-encrypt` tool.

use anyhow::Context;
use clap::{ArgGroup, Args, Parser, Subcommand, ValueEnum};
use lopdf::Permissions;
use pdf_encrypt::{
    decrypt_pdf_with_options, encrypt_pdf_with_options, read_pdf_properties, DecryptOptions,
    DocumentInfo, EncryptOptions, EncryptionAlgorithm, PdfCryptoReport,
};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const ROOT_AFTER_HELP: &str = r#"AI AGENT GUIDELINES (AI 代理執行與互動規範):
  1. 密碼詢問原則 (Request User Input for Passwords):
     - encrypt 至少須提供 --open-password (-p) 或 --owner-password 其中之一。
     - 若使用者未提供密碼，AI Agent 必須先向使用者詢問，切勿自行猜測、隨機產生或硬編碼密碼。
     - 未另行指定加密需求時，採用下方預設加密流程；若使用者要求開啟密碼，則使用 --open-password (-p)。
     - decrypt 解密需要開啟密碼的 PDF 時，若使用者未提供密碼，必須先向使用者詢問 --password (-p)。
     - 重新加密既有加密 PDF 時，若未知舊密碼，必須先向使用者詢問 --input-open-password。

  2. 保護原檔與避免非互動式卡死 (Preserve Originals & Avoid Stdin Hangs):
     - 預設應保留原始 PDF（不加 --in-place，輸出至 -o <FILE> 或預設 <原檔名>_encrypted.pdf / <原檔名>_decrypted.pdf）。
     - 除非使用者明確要求覆蓋原檔，否則請勿使用 --in-place，亦請勿將 --output 指向與 --input 相同的路徑。
     - [重要] encrypt 在輸出檔案已存在，或使用 --in-place 且未加 --force (-f) 時，會在終端機互動提示：
       「即將覆蓋檔案：...，是否繼續？[y/N] 」並等待 stdin 輸入。
     - AI Agent 於非互動環境執行前，應先檢查輸出路徑是否已存在；若需覆蓋既有檔案或使用 --in-place，
       必須先向使用者確認，取得同意後再加上 --force (-f) 執行，避免卡死在 stdin 提示。

  3. 機密性與權限限制說明 (Confidentiality vs. Viewer Permissions):
     - PDF 權限 (--read-only / --allow / --deny) 為閱讀器應遵守的限制，並非絕對防破解機制。
     - 若需保護文件機密性，請務必設定強密碼 (--open-password) 並使用預設的 --algorithm aes-256。

  4. 預設加密流程與欄位詢問 (Default Workflow & Request User Input):
     - 使用者未另行指定時，AI Agent 應預設採用下方 DEFAULT ENCRYPT WORKFLOW：免開啟密碼、
       使用 --owner-password 與 --deny modify --deny assemble，另存新檔並寫入全部八個文件資訊欄位。
     - 先執行 pdf-encrypt info -i "$PDF_INPUT" --json，唯讀查詢使用者提供的 PDF 文件屬性（Document Info 與 XMP），
       作為下列文件資訊欄位的預設值；使用者已明確指定的值優先於原檔屬性：
       --doc-title 標題、--doc-author 作者、--doc-subject 主旨、--doc-keywords 關鍵字；
       --doc-description 描述、--doc-copyright-status 著作權狀態、--doc-copyright-notice 著作權聲明、
       --doc-copyright-info-url 著作權資訊網址。
     - 將讀到的值預填成一份欄位清單，請使用者一次確認，避免逐欄要求重新輸入；只請使用者補充
       缺少、無法讀取、Document Info 與 XMP 互有衝突的欄位，或提供要修改的值。
     - 原屬性為空白時，可預填保留空白供使用者一併確認，例如 --doc-keywords ""；
       JSON 的 null 表示未取得值，"" 表示空字串；檢查 info_status、xmp_status、conflicts 與 warnings，
       無法讀取的屬性須標示為未讀取，不得當成空白，也不得自行推測或沿用範例值。
     - 未指定 --output (-o) 時，可將 <原檔名>_encrypted.pdf 作為預設輸出路徑一併確認。
       若尚未提供輸入路徑或 --owner-password，仍必須詢問；密碼不得由文件屬性推測或自行產生。
     - 下方 $PDF_* 與 $DOC_* 均為佔位變數，必須先設定為使用者提供或確認的預填值才能執行。

QUICK REFERENCE (指令與密碼／權限規則速覽):
  * info 文件屬性查詢：
    - pdf-encrypt info -i report.pdf --json：唯讀輸出文件資訊預設值、兩個來源及衝突／警告。
    - 需要密碼的輸入檔加上 --password (-p)；免開啟密碼的 PDF 可省略。properties 是 info 的別名。

  * encrypt 密碼語意：
    - 僅指定 -p, --open-password <PW>      : 開啟輸出 PDF 需輸入 <PW>；擁有者密碼自動設為相同 <PW>。
    - 僅指定 --owner-password <PW>         : 開啟輸出 PDF 不需密碼；用於搭配 --read-only 或 --allow/--deny 限制權限。
    - 同時指定 --open-password 與 --owner-password : 一般開啟受權限限制，輸入擁有者密碼可取得完整權限。
    - 指定 --input-open-password <OLD_PW>  : 先以 <OLD_PW> 解開已加密輸入檔，再以新設定重新加密。

  * encrypt 權限計算順序：
    1. 未指定 --allow 時預設允許全部權限 (all)；若指定 --allow <PERM>（可重複），則僅允許列出的權限。
    2. 若指定 --read-only (或 -ro)，等同於 --allow all --deny modify --deny assemble。
    3. 最後依序移除所有 --deny <PERM>（可重複）指定的權限。
    - 可用 <PERM> 值：all, print, high-quality-print, modify, copy, annotate, fill-forms, accessibility, assemble

  * decrypt 解密規則：
    - 支援以開啟密碼 (user/open password) 或擁有者密碼 (owner password) 解密，並移除 /Encrypt 字典。
    - 若 PDF 為空開啟密碼（例如僅設 --owner-password 的唯讀 PDF），可省略 -p 或傳入 -p "" 直接解密。

DEFAULT ENCRYPT WORKFLOW (AI 代理預設加密用法；先讀取輸入 PDF 屬性並確認預填值):
  pdf-encrypt encrypt \
    -i "$PDF_INPUT" \
    -o "$PDF_OUTPUT" \
    --owner-password "$PDF_OWNER_PASSWORD" --deny modify --deny assemble \
    --doc-title "$DOC_TITLE" \
    --doc-author "$DOC_AUTHOR" \
    --doc-subject "$DOC_SUBJECT" \
    --doc-keywords "$DOC_KEYWORDS" \
    --doc-description "$DOC_DESCRIPTION" \
    --doc-copyright-status "$DOC_COPYRIGHT_STATUS" \
    --doc-copyright-notice "$DOC_COPYRIGHT_NOTICE" \
    --doc-copyright-info-url "$DOC_COPYRIGHT_INFO_URL"

COMMON EXAMPLES (常用範例):
  # 查詢文件屬性，供 AI 代理預填文件資訊
  pdf-encrypt info -i report.pdf --json

  # 1. 使用開啟密碼加密 PDF（預設 aes-256，未指定 -o 時預設輸出 report_encrypted.pdf）
  pdf-encrypt encrypt -i report.pdf -o report_locked.pdf -p "$PDF_OPEN_PASSWORD"

  # 2. 建立免開啟密碼的唯讀 PDF（開啟免密碼，但禁止修改內容與組合頁面）
  pdf-encrypt encrypt -i report.pdf -o report_readonly.pdf --owner-password "$PDF_OWNER_PASSWORD" --read-only

  # 3. 建立嚴格僅供檢視的 PDF（禁止修改、組合頁面、列印與複製）
  pdf-encrypt encrypt -i report.pdf -o report_view_only.pdf --owner-password "$PDF_OWNER_PASSWORD" \
    --read-only --deny print --deny high-quality-print --deny copy

  # 4. 重新加密已加密的 PDF（先用舊密碼開啟，再換成新密碼）
  pdf-encrypt encrypt -i old_locked.pdf -o new_locked.pdf \
    --input-open-password "$OLD_PASSWORD" -p "$NEW_PASSWORD"

  # 5. 加密前同時寫入 PDF Document Info 與 XMP metadata
  pdf-encrypt encrypt -i report.pdf -o report_locked.pdf -p "$PDF_OPEN_PASSWORD" \
    --doc-title "規格驅動開發實戰" --doc-author "Will 保哥" --doc-keywords "AI, SDD, Rust"

  # 6. 原地加密並覆蓋原檔（須先向使用者確認覆蓋後，再加 --in-place --force）
  pdf-encrypt encrypt -i report.pdf -p "$PDF_OPEN_PASSWORD" --in-place --force

  # 7. 解密已加密的 PDF（未指定 -o 時預設輸出 report_locked_decrypted.pdf）
  pdf-encrypt decrypt -i report_locked.pdf -o report_plain.pdf -p "$PDF_PASSWORD"

  # 8. 解密免開啟密碼的唯讀 PDF（移除權限限制）
  pdf-encrypt decrypt -i report_readonly.pdf -o report_unlocked.pdf

OUTPUT REPORT (執行結果報表欄位，可用於驗證執行結果):
  成功時於 stdout 輸出完成訊息與以下欄位：
  operation (encrypt|decrypt), algorithm (僅 encrypt), input_size, output_size,
  size_delta, input_encrypted (true|false), output_encrypted (true|false), pages, objects"#;

const ENCRYPT_AFTER_HELP: &str = r#"AI AGENT GUIDELINES FOR ENCRYPT (AI 代理加密執行規範):
  1. 詢問必要密碼 (Ask User for Passwords):
     - --open-password (-p) 與 --owner-password 至少必須提供一個。
     - 若使用者未提供密碼，請先詢問使用者要設定的密碼，切勿自行假設或隨機編造密碼。
     - 未另行指定加密需求時，採用下方預設加密流程；若使用者要求開啟密碼，則使用 --open-password (-p)。
     - 若輸入 PDF 本身已加密，必須提供 --input-open-password <OLD_PASSWORD>；若未知舊密碼請先詢問使用者。

  2. 避免覆蓋確認提示導致非互動環境卡死 (Avoid Interactive [y/N] Stdin Hang):
     - 當輸出檔案已存在，或使用 --in-place 且未傳入 --force (-f) 時，CLI 會在 stderr 提示：
       「即將覆蓋檔案：...，是否繼續？[y/N] 」並等待 stdin 輸入。
     - 預設請保留原檔（寫入新檔案，勿加 --in-place）。
     - 若目標輸出檔已存在，或使用者要求 --in-place 覆蓋原檔，請先向使用者確認是否覆蓋；
       確認後務必加上 --force (-f) 以非互動方式執行。

  3. 預設加密流程與欄位詢問 (Default Workflow & Request User Input):
     - 使用者未另行指定時，AI Agent 應預設採用下方 DEFAULT ENCRYPT WORKFLOW：免開啟密碼、
       使用 --owner-password 與 --deny modify --deny assemble，另存新檔並寫入全部八個文件資訊欄位。
     - 先執行 pdf-encrypt info -i "$PDF_INPUT" --json，唯讀查詢使用者提供的 PDF 文件屬性（Document Info 與 XMP），
       作為下列文件資訊欄位的預設值；使用者已明確指定的值優先於原檔屬性：
       --doc-title 標題、--doc-author 作者、--doc-subject 主旨、--doc-keywords 關鍵字；
       --doc-description 描述、--doc-copyright-status 著作權狀態、--doc-copyright-notice 著作權聲明、
       --doc-copyright-info-url 著作權資訊網址。
     - 將讀到的值預填成一份欄位清單，請使用者一次確認，避免逐欄要求重新輸入；只請使用者補充
       缺少、無法讀取、Document Info 與 XMP 互有衝突的欄位，或提供要修改的值。
     - 原屬性為空白時，可預填保留空白供使用者一併確認，例如 --doc-keywords ""；
       JSON 的 null 表示未取得值，"" 表示空字串；檢查 info_status、xmp_status、conflicts 與 warnings，
       無法讀取的屬性須標示為未讀取，不得當成空白，也不得自行推測或沿用範例值。
     - 未指定 --output (-o) 時，可將 <原檔名>_encrypted.pdf 作為預設輸出路徑一併確認。
       若尚未提供輸入路徑或 --owner-password，仍必須詢問；密碼不得由文件屬性推測或自行產生。
     - 下方 $PDF_* 與 $DOC_* 均為佔位變數，必須先設定為使用者提供或確認的預填值才能執行。

PASSWORD & PERMISSION RULES (密碼與權限詳細規則):
  * 密碼組合行為：
    - 僅 -p, --open-password <PW>      : 開啟 PDF 需密碼 <PW>，且擁有者密碼自動設為 <PW>。
    - 僅 --owner-password <PW>         : 開啟 PDF 免密碼（空字串），適合搭配 --read-only 或 --deny 限制檢視者權限。
    - 同時指定兩者                      : 開啟時輸入 open-password 受權限限制；輸入 owner-password 享完整權限。

  * 權限評估順序：
    1. 預設允許全部權限 (all)；若指定一或多個 --allow <PERM>（且未指定 --read-only），則白名單僅允許列出的權限。
    2. 若指定 --read-only (或別名 -ro)，權限設為允許全部並移除 modify 與 assemble
       （等同 --allow all --deny modify --deny assemble）。
    3. 最後從目前權限集合中移除所有 --deny <PERM> 指定的項目。

  * 權限值 (PERMISSION) 一覽：
    - all                : 全部權限
    - print              : 允許列印文件
    - high-quality-print : 允許高品質列印
    - modify             : 允許修改內容
    - copy               : 允許複製或擷取文字與內容
    - annotate           : 允許新增或修改註解
    - fill-forms         : 允許填寫表單欄位
    - accessibility      : 允許輔助使用需求的內容擷取
    - assemble           : 允許插入、旋轉、刪除頁面或組合書籤大綱

  * 演算法 (ALGORITHM) 一覽：
    - aes-256 (預設) : PDF 2.0 AES-256 標準安全處理器（建議新文件使用）
    - aes-128        : PDF 1.5 AES-128 crypt filter（舊版閱讀器相容性用途）
    - rc4-128        : PDF 1.4 RC4 128-bit（舊系統相容性用途）
    - rc4-40         : PDF 1.4 RC4 40-bit（極舊系統相容性用途，不建議新文件使用）

  * 文件資訊 (Metadata) 參數對應：
    - --doc-title               : Document Info Title / XMP dc:title
    - --doc-author              : Document Info Author / XMP dc:creator
    - --doc-subject             : Document Info Subject / XMP pdfencrypt:Subject
    - --doc-keywords            : Document Info Keywords / XMP pdf:Keywords & dc:subject（逗號或分號分隔）
    - --doc-description         : Document Info Description / XMP dc:description
    - --doc-copyright-status    : CopyrightStatus / XMP xmpRights:Marked（常用值：Copyrighted 或 Public Domain）
    - --doc-copyright-notice    : CopyrightNotice / XMP dc:rights
    - --doc-copyright-info-url  : CopyrightInfoURL / XMP xmpRights:WebStatement

DEFAULT ENCRYPT WORKFLOW (AI 代理預設加密用法；先讀取輸入 PDF 屬性並確認預填值):
  pdf-encrypt encrypt \
    -i "$PDF_INPUT" \
    -o "$PDF_OUTPUT" \
    --owner-password "$PDF_OWNER_PASSWORD" --deny modify --deny assemble \
    --doc-title "$DOC_TITLE" \
    --doc-author "$DOC_AUTHOR" \
    --doc-subject "$DOC_SUBJECT" \
    --doc-keywords "$DOC_KEYWORDS" \
    --doc-description "$DOC_DESCRIPTION" \
    --doc-copyright-status "$DOC_COPYRIGHT_STATUS" \
    --doc-copyright-notice "$DOC_COPYRIGHT_NOTICE" \
    --doc-copyright-info-url "$DOC_COPYRIGHT_INFO_URL"

EXAMPLES (常用加密範例):
  # 基本開啟密碼加密（預設 aes-256）
  pdf-encrypt encrypt -i report.pdf -o report_locked.pdf -p "$PDF_OPEN_PASSWORD"

  # 分別設定開啟密碼與擁有者密碼，並設為唯讀
  pdf-encrypt encrypt -i report.pdf -o report_readonly.pdf \
    --open-password "$PDF_OPEN_PASSWORD" --owner-password "$PDF_OWNER_PASSWORD" --read-only

  # 免開啟密碼的唯讀 PDF（可用 --read-only 或 -ro）
  pdf-encrypt encrypt -i report.pdf -o report_readonly.pdf --owner-password "$PDF_OWNER_PASSWORD" -ro

  # 嚴格僅供閱讀（禁止修改、重組頁面、列印、複製）
  pdf-encrypt encrypt -i report.pdf -o report_view_only.pdf --owner-password "$PDF_OWNER_PASSWORD" \
    --read-only --deny print --deny high-quality-print --deny copy

  # 僅允許列印與輔助擷取（白名單模式）
  pdf-encrypt encrypt -i report.pdf -o report_print_only.pdf -p "$PDF_OPEN_PASSWORD" \
    --allow print --allow high-quality-print --allow accessibility

  # 重新加密已加密的 PDF
  pdf-encrypt encrypt -i old_locked.pdf -o new_locked.pdf \
    --input-open-password "$OLD_PASSWORD" -p "$NEW_PASSWORD" --algorithm aes-256

  # 原地加密覆蓋原檔（須經使用者同意後加上 --force 避免互動提示卡住）
  pdf-encrypt encrypt -i report.pdf -p "$PDF_OPEN_PASSWORD" --in-place --force"#;

const DECRYPT_AFTER_HELP: &str = r#"AI AGENT GUIDELINES FOR DECRYPT (AI 代理常規解密規範):
  1. 詢問解密密碼 (Ask User for Password):
     - 若加密 PDF 需要密碼才能開啟，且使用者尚未提供密碼，AI Agent 必須先向使用者詢問 --password (-p)，
       切勿自行猜測密碼。
     - 若 PDF 是「免開啟密碼」的權限受限／唯讀文件（即加密時僅設定 --owner-password，開啟密碼為空），
       可省略 --password（預設為空字串 ""）直接解密並移除權限限制，或傳入擁有者密碼解密。

  2. 保留原始加密檔案 (Preserve Original File):
     - 未指定 --output (-o) 時，預設輸出至 <原檔名>_decrypted.pdf，不會覆蓋原檔。
     - 請勿將 --output 設為與 --input 相同的檔案路徑，以免損毀原始檔案。

  3. 錯誤行為與驗證 (Error Behavior & Verification):
     - 若輸入 PDF 本身並未加密，或密碼錯誤，指令會失敗並回傳非零退出碼 (exit code 1)，且不會寫出錯誤輸出檔。
     - 成功解密後請檢查 stdout 報表：確認 operation: decrypt、input_encrypted: true、output_encrypted: false，
       且輸出檔案存在並具有非零大小 (output_size > 0)。

EXAMPLES (常用解密範例):
  # 使用開啟密碼或擁有者密碼解密 PDF（指定輸出路徑）
  pdf-encrypt decrypt -i encrypted.pdf -o decrypted.pdf -p "$PDF_PASSWORD"

  # 解密並使用預設輸出檔名（自動輸出至 report_encrypted_decrypted.pdf）
  pdf-encrypt decrypt -i report_encrypted.pdf -p "$PDF_PASSWORD"

  # 解密免開啟密碼（空密碼）的唯讀／權限受限 PDF
  pdf-encrypt decrypt -i report_readonly.pdf -o report_unlocked.pdf"#;

const INFO_AFTER_HELP: &str = r#"AI AGENT GUIDELINES FOR INFO (AI 代理文件屬性查詢規範):
  - 此指令只讀取檔案，不會修改原檔或建立解密副本；適用於其他工具產生的 PDF。
  - AI 代理應先查詢輸入 PDF 的屬性，以 document_info 預填 --doc-* 欄位，再一次請使用者確認，
    只補充缺少、衝突、無法讀取或需要修改的值；使用者明確指定的值優先。
  - document_info 優先採用 Document Info，缺值再取 XMP；保留原本空字串，不將它視為缺值。
  - info_dictionary 列出可讀的標準及自訂欄位；xmp_properties 以 {namespace URI}local-name 為鍵。
  - null 表示未取得值，"" 表示空字串；info_status / xmp_status 可為 absent、read、partial 或 unreadable。
    必須一併檢查 conflicts 與 warnings，不得將解析失敗當成欄位空白。
  - 需要密碼才能開啟的 PDF，必須提供正確的 --password (-p)；缺少密碼時先詢問使用者。
    AES-256 可用開啟密碼或擁有者密碼；AES-128／RC4 若開啟密碼非空，須提供開啟密碼，
    目前 PDF 相依套件無法可靠地以舊版擁有者密碼解密，指令會拒絕回傳可能錯誤的屬性。
    不支援的加密方式或無法解析的檔案會失敗；錯誤會寫入 stderr 並回傳非零退出碼。

EXAMPLES (文件屬性查詢範例):
  pdf-encrypt info -i report.pdf
  pdf-encrypt info -i report.pdf --json
  pdf-encrypt info -i encrypted.pdf --password "$PDF_PASSWORD" --json
  pdf-encrypt properties -i report.pdf --json"#;

/// PDF 加密與解密命令列工具。
#[derive(Debug, Parser)]
#[command(
    name = "pdf-encrypt",
    version,
    about = "PDF 加密、解密、權限設定與文件屬性讀寫工具（無外部 PDF 工具依賴）",
    long_about = None,
    after_help = ROOT_AFTER_HELP,
    subcommand_required = true,
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// 唯讀查詢 PDF 文件屬性、Document Info 與 XMP；可輸出 JSON（別名 properties）
    #[command(visible_alias = "properties")]
    Info(InfoArgs),
    /// 將 PDF 加密、設定檢視權限或更新 Metadata 並寫出檔案（或以 --in-place 覆蓋原檔）
    Encrypt(Box<EncryptArgs>),
    /// 將已加密 PDF 解密（移除密碼與權限限制）並寫出新檔案
    Decrypt(DecryptArgs),
}

#[derive(Debug, Args)]
#[command(after_help = INFO_AFTER_HELP)]
struct InfoArgs {
    /// 要讀取文件屬性的 PDF 檔案路徑
    #[arg(short, long, value_name = "FILE")]
    input: PathBuf,

    /// 輸入 PDF 的開啟密碼或擁有者密碼；未指定時嘗試空密碼
    #[arg(short = 'p', long, value_name = "PASSWORD")]
    password: Option<String>,

    /// 輸出 JSON，供 AI 代理或其他程式讀取；不夾帶完成訊息
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
#[command(
    after_help = ENCRYPT_AFTER_HELP,
    group(
        ArgGroup::new("encrypt_password")
            .required(true)
            .multiple(true)
            .args(["open_password", "owner_password"])
    )
)]
struct EncryptArgs {
    /// 輸入 PDF 檔案路徑
    #[arg(short, long, value_name = "FILE")]
    input: PathBuf,

    /// 輸出 PDF 檔案路徑（預設為 <輸入檔名>_encrypted.pdf；不可與 --in-place 同時使用）
    #[arg(short, long, value_name = "FILE", conflicts_with = "in_place")]
    output: Option<PathBuf>,

    /// 直接加密並覆蓋原輸入檔案；會先於同目錄寫入暫存檔，成功後才取代原檔（不可與 --output 同時使用）
    #[arg(long = "in-place", conflicts_with = "output")]
    in_place: bool,

    /// 覆蓋既有輸出檔或使用 --in-place 時，略過 [y/N] 互動確認提示（非互動環境覆蓋時必加）
    #[arg(short, long)]
    force: bool,

    /// 加密後開啟輸出 PDF 所需的密碼；未指定 --owner-password 時也自動作為擁有者密碼
    #[arg(short = 'p', long, value_name = "PASSWORD")]
    open_password: Option<String>,

    /// 擁有者密碼（完整權限密碼）；未指定 --open-password 時，輸出 PDF 開啟免密碼但套用權限限制
    #[arg(long, value_name = "PASSWORD")]
    owner_password: Option<String>,

    /// 輸入檔若已加密，先用此密碼開啟輸入檔後再重新加密
    #[arg(long, value_name = "PASSWORD")]
    input_open_password: Option<String>,

    /// 將開啟文件後的權限設為唯讀（等同 --allow all --deny modify --deny assemble）；亦可使用簡寫 -ro
    #[arg(long = "read-only")]
    read_only: bool,

    /// 加密演算法
    #[arg(long, value_enum, default_value_t = CliAlgorithm::Aes256)]
    algorithm: CliAlgorithm,

    /// 只允許指定權限（白名單模式，可重複指定）；未指定時預設允許全部權限 (all)
    #[arg(long = "allow", value_enum, value_name = "PERMISSION")]
    allow: Vec<CliPermission>,

    /// 從目前允許權限中移除指定權限（可重複指定；會在 --allow 或 --read-only 之後套用）
    #[arg(long = "deny", value_enum, value_name = "PERMISSION")]
    deny: Vec<CliPermission>,

    /// 文件標題；加密前寫入 Document Info (Title) 與 XMP (dc:title)
    #[arg(long = "doc-title", value_name = "TEXT")]
    doc_title: Option<String>,

    /// 文件作者；加密前寫入 Document Info (Author) 與 XMP (dc:creator)
    #[arg(long = "doc-author", value_name = "TEXT")]
    doc_author: Option<String>,

    /// 文件主旨；加密前寫入 Document Info (Subject) 與 XMP (pdfencrypt:Subject)
    #[arg(long = "doc-subject", value_name = "TEXT")]
    doc_subject: Option<String>,

    /// 文件關鍵字（可以逗號或分號分隔）；加密前寫入 Document Info (Keywords) 與 XMP (pdf:Keywords, dc:subject)
    #[arg(long = "doc-keywords", value_name = "TEXT")]
    doc_keywords: Option<String>,

    /// 文件描述；加密前寫入 Document Info (Description) 與 XMP (dc:description)
    #[arg(long = "doc-description", value_name = "TEXT")]
    doc_description: Option<String>,

    /// 文件著作權狀態（如 Copyrighted 或 Public Domain）；加密前寫入 Document Info 與 XMP (xmpRights:Marked)
    #[arg(long = "doc-copyright-status", value_name = "TEXT")]
    doc_copyright_status: Option<String>,

    /// 文件著作權聲明；加密前寫入 Document Info (CopyrightNotice) 與 XMP (dc:rights)
    #[arg(long = "doc-copyright-notice", value_name = "TEXT")]
    doc_copyright_notice: Option<String>,

    /// 文件著作權資訊 URL；加密前寫入 Document Info (CopyrightInfoURL) 與 XMP (xmpRights:WebStatement)
    #[arg(long = "doc-copyright-info-url", value_name = "URL")]
    doc_copyright_info_url: Option<String>,

    /// 不加密 PDF XMP metadata stream（預設會一併加密 metadata）
    #[arg(long = "no-encrypt-metadata")]
    no_encrypt_metadata: bool,
}

#[derive(Debug, Args)]
#[command(after_help = DECRYPT_AFTER_HELP)]
struct DecryptArgs {
    /// 已加密的輸入 PDF 檔案路徑
    #[arg(short, long, value_name = "FILE")]
    input: PathBuf,

    /// 解密後輸出 PDF 檔案路徑（預設為 <輸入檔名>_decrypted.pdf）
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// 使用者開啟密碼或擁有者密碼；未指定時預設使用空密碼 ""（適用於免開啟密碼之唯讀 PDF）
    #[arg(short = 'p', long, default_value = "", value_name = "PASSWORD")]
    password: String,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliAlgorithm {
    /// PDF 2.0 AES-256 標準安全處理器（預設，建議新文件使用）
    #[value(name = "aes-256")]
    Aes256,
    /// PDF 1.5 AES-128 crypt filter（適合較舊 PDF reader 相容性）
    #[value(name = "aes-128")]
    Aes128,
    /// PDF 1.4 RC4 128-bit（相容性用途）
    #[value(name = "rc4-128")]
    Rc4Bit128,
    /// PDF 1.4 RC4 40-bit（舊系統相容性用途，不建議用於新文件）
    #[value(name = "rc4-40")]
    Rc4Bit40,
}

impl From<CliAlgorithm> for EncryptionAlgorithm {
    fn from(value: CliAlgorithm) -> Self {
        match value {
            CliAlgorithm::Aes256 => EncryptionAlgorithm::Aes256,
            CliAlgorithm::Aes128 => EncryptionAlgorithm::Aes128,
            CliAlgorithm::Rc4Bit128 => EncryptionAlgorithm::Rc4Bit128,
            CliAlgorithm::Rc4Bit40 => EncryptionAlgorithm::Rc4Bit40,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliPermission {
    /// 全部權限
    All,
    /// 列印文件
    Print,
    /// 高品質列印
    #[value(name = "high-quality-print")]
    HighQualityPrint,
    /// 修改內容
    Modify,
    /// 複製或擷取內容
    Copy,
    /// 新增或修改註解
    Annotate,
    /// 填寫表單
    #[value(name = "fill-forms")]
    FillForms,
    /// 輔助使用需求的內容擷取
    Accessibility,
    /// 組合文件頁面或大綱
    Assemble,
}

impl CliPermission {
    fn permissions(self) -> Permissions {
        match self {
            Self::All => Permissions::all(),
            Self::Print => Permissions::PRINTABLE,
            Self::HighQualityPrint => Permissions::PRINTABLE_IN_HIGH_QUALITY,
            Self::Modify => Permissions::MODIFIABLE,
            Self::Copy => Permissions::COPYABLE,
            Self::Annotate => Permissions::ANNOTABLE,
            Self::FillForms => Permissions::FILLABLE,
            Self::Accessibility => Permissions::COPYABLE_FOR_ACCESSIBILITY,
            Self::Assemble => Permissions::ASSEMBLABLE,
        }
    }
}

fn main() {
    let cli = parse_cli_from(std::env::args_os()).unwrap_or_else(|err| err.exit());

    let result = match &cli.command {
        Command::Info(args) => run_info(args),
        Command::Encrypt(args) => run_encrypt(args),
        Command::Decrypt(args) => run_decrypt(args),
    };

    if let Err(err) = result {
        eprintln!("處理失敗：{err:#}");
        std::process::exit(1);
    }
}

fn run_info(args: &InfoArgs) -> anyhow::Result<()> {
    let properties = read_pdf_properties(&args.input, args.password.as_deref())?;
    let value = serde_json::to_value(&properties)?;
    let mut stdout = io::stdout().lock();
    if args.json {
        serde_json::to_writer_pretty(&mut stdout, &value)?;
        writeln!(stdout)?;
    } else {
        for key in [
            "pdf_version",
            "input_encrypted",
            "pages",
            "info_status",
            "xmp_status",
        ] {
            writeln!(stdout, "{key}: {}", value[key])?;
        }
        for key in ["document_info", "info_dictionary", "xmp_properties"] {
            writeln!(stdout, "{key}:")?;
            if let Some(fields) = value[key].as_object() {
                for (field, value) in fields {
                    writeln!(stdout, "  {field}: {value}")?;
                }
            }
        }
        writeln!(stdout, "conflicts: {}", value["conflicts"])?;
        writeln!(stdout, "warnings: {}", value["warnings"])?;
    }
    Ok(())
}

fn run_encrypt(args: &EncryptArgs) -> anyhow::Result<()> {
    let options = encrypt_options_from_args(args)?;

    if args.in_place {
        return run_encrypt_in_place(args, &options);
    }

    let output_path = args
        .output
        .clone()
        .unwrap_or_else(|| default_output_path(&args.input, "encrypted"));
    confirm_overwrite_if_needed(&output_path, args.force)?;
    let report = encrypt_pdf_with_options(&args.input, &output_path, &options)?;

    println!("已完成加密：{}", output_path.display());
    print_report(&report);
    Ok(())
}

fn run_encrypt_in_place(args: &EncryptArgs, options: &EncryptOptions) -> anyhow::Result<()> {
    confirm_overwrite(&args.input, args.force)?;

    let input_permissions = fs::metadata(&args.input)
        .with_context(|| format!("無法讀取輸入檔案 metadata：{}", args.input.display()))?
        .permissions();
    let temp_output_path = in_place_temp_output_path(&args.input);

    let report = match encrypt_pdf_with_options(&args.input, &temp_output_path, options) {
        Ok(report) => report,
        Err(err) => {
            remove_temp_file_if_exists(&temp_output_path);
            return Err(err);
        }
    };

    if let Err(err) = fs::set_permissions(&temp_output_path, input_permissions).with_context(|| {
        format!(
            "無法套用原輸入檔案權限到暫存檔：{}",
            temp_output_path.display()
        )
    }) {
        remove_temp_file_if_exists(&temp_output_path);
        return Err(err);
    }

    if let Err(err) = fs::rename(&temp_output_path, &args.input)
        .with_context(|| format!("無法用加密後暫存檔取代原輸入檔案：{}", args.input.display()))
    {
        remove_temp_file_if_exists(&temp_output_path);
        return Err(err);
    }

    println!("已完成加密並覆蓋原檔：{}", args.input.display());
    print_report(&report);
    Ok(())
}

fn confirm_overwrite_if_needed(path: &Path, force: bool) -> anyhow::Result<()> {
    if force || !path.exists() {
        return Ok(());
    }

    confirm_overwrite(path, force)
}

fn confirm_overwrite(path: &Path, force: bool) -> anyhow::Result<()> {
    if force {
        return Ok(());
    }

    eprint!("即將覆蓋檔案：{}，是否繼續？[y/N] ", path.display());
    io::stderr().flush().context("無法刷新覆蓋確認提示")?;

    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .context("無法讀取覆蓋確認回覆")?;

    match answer.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => Ok(()),
        _ => anyhow::bail!("已取消覆蓋：{}", path.display()),
    }
}

fn run_decrypt(args: &DecryptArgs) -> anyhow::Result<()> {
    let output_path = args
        .output
        .clone()
        .unwrap_or_else(|| default_output_path(&args.input, "decrypted"));
    let options = DecryptOptions::new(args.password.clone());
    let report = decrypt_pdf_with_options(&args.input, &output_path, &options)?;

    println!("已完成解密：{}", output_path.display());
    print_report(&report);
    Ok(())
}

fn parse_cli_from<I, T>(args: I) -> std::result::Result<Cli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    Cli::try_parse_from(
        args.into_iter()
            .map(|arg| normalize_read_only_alias(arg.into())),
    )
}

fn normalize_read_only_alias(arg: OsString) -> OsString {
    if arg == OsStr::new("-ro") {
        OsString::from("--read-only")
    } else {
        arg
    }
}

fn encrypt_options_from_args(args: &EncryptArgs) -> anyhow::Result<EncryptOptions> {
    let open_password = args.open_password.clone().unwrap_or_default();
    let owner_password = args
        .owner_password
        .clone()
        .unwrap_or_else(|| open_password.clone());

    let mut options = EncryptOptions::with_open_password(open_password);
    options.owner_password = owner_password;
    options.input_open_password = args.input_open_password.clone();
    options.document_info = document_info_from_args(args);
    options.algorithm = args.algorithm.into();
    options.permissions =
        permissions_from_args(&args.allow, &args.deny, options.permissions, args.read_only);
    options.encrypt_metadata = !args.no_encrypt_metadata;
    Ok(options)
}

fn document_info_from_args(args: &EncryptArgs) -> DocumentInfo {
    DocumentInfo {
        title: args.doc_title.clone(),
        author: args.doc_author.clone(),
        subject: args.doc_subject.clone(),
        keywords: args.doc_keywords.clone(),
        description: args.doc_description.clone(),
        copyright_status: args.doc_copyright_status.clone(),
        copyright_notice: args.doc_copyright_notice.clone(),
        copyright_info_url: args.doc_copyright_info_url.clone(),
    }
}

fn permissions_from_args(
    allow: &[CliPermission],
    deny: &[CliPermission],
    default_permissions: Permissions,
    read_only: bool,
) -> Permissions {
    let mut permissions = if read_only {
        Permissions::all()
    } else if allow.is_empty() {
        default_permissions
    } else {
        allow
            .iter()
            .fold(Permissions::empty(), |acc, item| acc | item.permissions())
    };

    if read_only {
        permissions &= !Permissions::MODIFIABLE;
        permissions &= !Permissions::ASSEMBLABLE;
    }

    for item in deny {
        permissions &= !item.permissions();
    }

    permissions
}

fn default_output_path(input: &Path, suffix: &str) -> PathBuf {
    let mut path = input.to_path_buf();
    let file_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let mut new_name = format!("{file_name}_{suffix}");
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        new_name.push('.');
        new_name.push_str(ext);
    }
    path.set_file_name(new_name);
    path
}

fn in_place_temp_output_path(input: &Path) -> PathBuf {
    let parent = input
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = input
        .file_name()
        .unwrap_or_else(|| OsStr::new("output.pdf"));
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();

    let mut temp_file_name = OsString::from(".");
    temp_file_name.push(file_name);
    temp_file_name.push(format!(
        ".pdf-encrypt-{}-{timestamp}.tmp",
        std::process::id()
    ));
    parent.join(temp_file_name)
}

fn remove_temp_file_if_exists(path: &Path) {
    let _ = fs::remove_file(path);
}

fn print_report(report: &PdfCryptoReport) {
    println!("operation: {}", report.operation.label());
    if let Some(algorithm) = report.algorithm {
        println!("algorithm: {}", algorithm.label());
    }
    println!("input_size: {}", report.input_size);
    println!("output_size: {}", report.output_size);
    println!("size_delta: {}", report.size_delta());
    println!("input_encrypted: {}", report.input_encrypted);
    println!("output_encrypted: {}", report.output_encrypted);
    println!("pages: {}", report.page_count);
    println!("objects: {}", report.object_count);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_encrypt_output_uses_encrypted_suffix() {
        let output = default_output_path(Path::new("report.pdf"), "encrypted");
        assert_eq!(output, PathBuf::from("report_encrypted.pdf"));
    }

    #[test]
    fn in_place_temp_output_stays_next_to_input() {
        let temp_path = in_place_temp_output_path(Path::new("docs/report.pdf"));

        assert_eq!(temp_path.parent(), Some(Path::new("docs")));
        assert!(temp_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(
                |name| name.starts_with(".report.pdf.pdf-encrypt-") && name.ends_with(".tmp")
            ));
    }

    #[test]
    fn in_place_flag_is_parsed() {
        let cli = Cli::try_parse_from([
            "pdf-encrypt",
            "encrypt",
            "--input",
            "input.pdf",
            "--owner-password",
            "owner",
            "--in-place",
        ])
        .unwrap();
        let Command::Encrypt(args) = cli.command else {
            panic!("expected encrypt command");
        };

        assert!(args.in_place);
        assert!(args.output.is_none());
    }

    #[test]
    fn force_flag_is_parsed() {
        let cli = Cli::try_parse_from([
            "pdf-encrypt",
            "encrypt",
            "--input",
            "input.pdf",
            "--owner-password",
            "owner",
            "--force",
        ])
        .unwrap();
        let Command::Encrypt(args) = cli.command else {
            panic!("expected encrypt command");
        };

        assert!(args.force);
    }

    #[test]
    fn in_place_conflicts_with_output() {
        let result = Cli::try_parse_from([
            "pdf-encrypt",
            "encrypt",
            "--input",
            "input.pdf",
            "--output",
            "output.pdf",
            "--owner-password",
            "owner",
            "--in-place",
        ]);

        assert!(result.is_err());
    }

    #[test]
    fn allow_list_replaces_default_permissions() {
        let permissions = permissions_from_args(
            &[CliPermission::Print, CliPermission::Copy],
            &[],
            Permissions::all(),
            false,
        );

        assert!(permissions.contains(Permissions::PRINTABLE));
        assert!(permissions.contains(Permissions::COPYABLE));
        assert!(!permissions.contains(Permissions::MODIFIABLE));
    }

    #[test]
    fn deny_list_removes_permissions_from_default() {
        let permissions =
            permissions_from_args(&[], &[CliPermission::Copy], Permissions::all(), false);

        assert!(!permissions.contains(Permissions::COPYABLE));
        assert!(permissions.contains(Permissions::PRINTABLE));
    }

    #[test]
    fn open_password_sets_output_open_password_and_default_owner_password() {
        let cli = Cli::try_parse_from([
            "pdf-encrypt",
            "encrypt",
            "--input",
            "input.pdf",
            "--open-password",
            "secret",
        ])
        .unwrap();
        let Command::Encrypt(args) = cli.command else {
            panic!("expected encrypt command");
        };
        let options = encrypt_options_from_args(&args).unwrap();

        assert_eq!(options.user_password, "secret");
        assert_eq!(options.owner_password, "secret");
        assert_eq!(options.permissions, Permissions::all());
    }

    #[test]
    fn owner_password_overrides_default_owner_password() {
        let cli = Cli::try_parse_from([
            "pdf-encrypt",
            "encrypt",
            "--input",
            "input.pdf",
            "--open-password",
            "viewer",
            "--owner-password",
            "owner",
        ])
        .unwrap();
        let Command::Encrypt(args) = cli.command else {
            panic!("expected encrypt command");
        };
        let options = encrypt_options_from_args(&args).unwrap();

        assert_eq!(options.user_password, "viewer");
        assert_eq!(options.owner_password, "owner");
    }

    #[test]
    fn owner_password_without_open_password_leaves_output_open_password_empty() {
        let cli = Cli::try_parse_from([
            "pdf-encrypt",
            "encrypt",
            "--input",
            "input.pdf",
            "--owner-password",
            "owner",
        ])
        .unwrap();
        let Command::Encrypt(args) = cli.command else {
            panic!("expected encrypt command");
        };
        let options = encrypt_options_from_args(&args).unwrap();

        assert_eq!(options.user_password, "");
        assert_eq!(options.owner_password, "owner");
    }

    #[test]
    fn input_open_password_sets_password_for_encrypted_input() {
        let cli = Cli::try_parse_from([
            "pdf-encrypt",
            "encrypt",
            "--input",
            "input.pdf",
            "--input-open-password",
            "old-secret",
            "--open-password",
            "new-secret",
        ])
        .unwrap();
        let Command::Encrypt(args) = cli.command else {
            panic!("expected encrypt command");
        };
        let options = encrypt_options_from_args(&args).unwrap();

        assert_eq!(options.input_open_password.as_deref(), Some("old-secret"));
        assert_eq!(options.user_password, "new-secret");
    }

    #[test]
    fn document_info_args_populate_encrypt_options() {
        let cli = Cli::try_parse_from([
            "pdf-encrypt",
            "encrypt",
            "--input",
            "input.pdf",
            "--open-password",
            "secret",
            "--doc-title",
            "規格驅動開發實戰",
            "--doc-author",
            "Will 保哥",
            "--doc-subject",
            "AI 時代的軟體開發新典範",
            "--doc-keywords",
            "AI, SDD, Rust",
            "--doc-description",
            "課程講義",
            "--doc-copyright-status",
            "Copyrighted",
            "--doc-copyright-notice",
            "Copyright © Will 保哥",
            "--doc-copyright-info-url",
            "https://example.com/copyright",
        ])
        .unwrap();
        let Command::Encrypt(args) = cli.command else {
            panic!("expected encrypt command");
        };
        let options = encrypt_options_from_args(&args).unwrap();

        assert_eq!(
            options.document_info.title.as_deref(),
            Some("規格驅動開發實戰")
        );
        assert_eq!(options.document_info.author.as_deref(), Some("Will 保哥"));
        assert_eq!(
            options.document_info.subject.as_deref(),
            Some("AI 時代的軟體開發新典範")
        );
        assert_eq!(
            options.document_info.keywords.as_deref(),
            Some("AI, SDD, Rust")
        );
        assert_eq!(
            options.document_info.description.as_deref(),
            Some("課程講義")
        );
        assert_eq!(
            options.document_info.copyright_status.as_deref(),
            Some("Copyrighted")
        );
        assert_eq!(
            options.document_info.copyright_notice.as_deref(),
            Some("Copyright © Will 保哥")
        );
        assert_eq!(
            options.document_info.copyright_info_url.as_deref(),
            Some("https://example.com/copyright")
        );
    }

    #[test]
    fn read_only_denies_modify_and_assemble_from_all_permissions() {
        let cli = Cli::try_parse_from([
            "pdf-encrypt",
            "encrypt",
            "--input",
            "input.pdf",
            "--open-password",
            "viewer",
            "--read-only",
        ])
        .unwrap();
        let Command::Encrypt(args) = cli.command else {
            panic!("expected encrypt command");
        };
        let options = encrypt_options_from_args(&args).unwrap();

        assert!(options.permissions.contains(Permissions::PRINTABLE));
        assert!(options.permissions.contains(Permissions::COPYABLE));
        assert!(options.permissions.contains(Permissions::ANNOTABLE));
        assert!(options.permissions.contains(Permissions::FILLABLE));
        assert!(!options.permissions.contains(Permissions::ASSEMBLABLE));
        assert!(!options.permissions.contains(Permissions::MODIFIABLE));
    }

    #[test]
    fn read_only_accepts_dash_ro_alias() {
        let cli = parse_cli_from([
            "pdf-encrypt",
            "encrypt",
            "--input",
            "input.pdf",
            "--open-password",
            "viewer",
            "-ro",
        ])
        .unwrap();
        let Command::Encrypt(args) = cli.command else {
            panic!("expected encrypt command");
        };

        assert!(args.read_only);
    }

    #[test]
    fn encrypt_requires_open_or_owner_password() {
        let result = Cli::try_parse_from(["pdf-encrypt", "encrypt", "--input", "input.pdf"]);

        assert!(result.is_err());
    }

    #[test]
    fn root_short_and_long_help_are_self_contained_for_ai_agents() {
        for flag in ["-h", "--help"] {
            let err = parse_cli_from(["pdf-encrypt", flag]).unwrap_err();
            let rendered = err.to_string();

            assert!(rendered.contains("AI AGENT GUIDELINES"));
            assert!(rendered.contains("QUICK REFERENCE"));
            assert!(rendered.contains("COMMON EXAMPLES"));
            assert!(rendered.contains("OUTPUT REPORT"));
            assert!(rendered.contains("--open-password"));
            assert!(rendered.contains("--owner-password"));
            assert!(rendered.contains("--force"));
            assert!(rendered.contains("pdf-encrypt encrypt"));
            assert!(rendered.contains("pdf-encrypt decrypt"));
        }
    }

    #[test]
    fn encrypt_short_and_long_help_are_self_contained_for_ai_agents() {
        for flag in ["-h", "--help"] {
            let err = parse_cli_from(["pdf-encrypt", "encrypt", flag]).unwrap_err();
            let rendered = err.to_string();

            assert!(rendered.contains("AI AGENT GUIDELINES FOR ENCRYPT"));
            assert!(rendered.contains("PASSWORD & PERMISSION RULES"));
            assert!(rendered.contains("EXAMPLES"));
            assert!(rendered.contains("--input-open-password"));
            assert!(rendered.contains("--read-only"));
            assert!(rendered.contains("-ro"));
            assert!(rendered.contains("--in-place"));
            assert!(rendered.contains("--force"));
        }
    }

    #[test]
    fn decrypt_short_and_long_help_are_self_contained_for_ai_agents() {
        for flag in ["-h", "--help"] {
            let err = parse_cli_from(["pdf-encrypt", "decrypt", flag]).unwrap_err();
            let rendered = err.to_string();

            assert!(rendered.contains("AI AGENT GUIDELINES FOR DECRYPT"));
            assert!(rendered.contains("EXAMPLES"));
            assert!(rendered.contains("--password"));
            assert!(rendered.contains("_decrypted.pdf"));
            assert!(rendered.contains("output_encrypted: false"));
        }
    }
}
