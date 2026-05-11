//! Command line interface for the `pdf-encrypt` tool.

use anyhow::Context;
use clap::{ArgGroup, Args, Parser, Subcommand, ValueEnum};
use lopdf::Permissions;
use pdf_encrypt::{
    decrypt_pdf_with_options, encrypt_pdf_with_options, DecryptOptions, DocumentInfo,
    EncryptOptions, EncryptionAlgorithm, PdfCryptoReport,
};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// PDF 加密與解密命令列工具。
#[derive(Debug, Parser)]
#[command(
    name = "pdf-encrypt",
    version,
    about = "PDF 加密與解密工具",
    long_about = None,
    subcommand_required = true,
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// 將 PDF 加密並寫出新檔案
    Encrypt(Box<EncryptArgs>),
    /// 將已加密 PDF 解密並寫出新檔案
    Decrypt(DecryptArgs),
}

#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("encrypt_password")
        .required(true)
        .multiple(true)
        .args(["open_password", "owner_password"])
))]
struct EncryptArgs {
    /// 輸入 PDF 檔案路徑
    #[arg(short, long, value_name = "FILE")]
    input: PathBuf,

    /// 輸出 PDF 檔案路徑 (預設為 input 加密字尾)
    #[arg(short, long, value_name = "FILE", conflicts_with = "in_place")]
    output: Option<PathBuf>,

    /// 直接加密並覆蓋原輸入檔案；會先寫入暫存檔，成功後才取代原檔
    #[arg(long = "in-place", conflicts_with = "output")]
    in_place: bool,

    /// 加密後開啟輸出 PDF 所需的密碼；未指定 --owner-password 時也作為擁有者密碼
    #[arg(short = 'p', long, value_name = "PASSWORD")]
    open_password: Option<String>,

    /// 擁有者密碼；未指定 --open-password 時，輸出 PDF 不需要開啟密碼
    #[arg(long, value_name = "PASSWORD")]
    owner_password: Option<String>,

    /// 輸入檔若已加密，用此密碼先開啟再重新加密
    #[arg(long, value_name = "PASSWORD")]
    input_open_password: Option<String>,

    /// 將開啟文件後的權限設為 allow all 並 deny modify；也可使用 -ro
    #[arg(long = "read-only")]
    read_only: bool,

    /// 加密演算法
    #[arg(long, value_enum, default_value_t = CliAlgorithm::Aes256)]
    algorithm: CliAlgorithm,

    /// 只允許指定權限；可重複。未指定時預設允許全部權限
    #[arg(long = "allow", value_enum, value_name = "PERMISSION")]
    allow: Vec<CliPermission>,

    /// 從目前允許權限中移除指定權限；可重複
    #[arg(long = "deny", value_enum, value_name = "PERMISSION")]
    deny: Vec<CliPermission>,

    /// 文件標題；加密前寫入 Document Info / XMP metadata
    #[arg(long = "doc-title", value_name = "TEXT")]
    doc_title: Option<String>,

    /// 文件作者；加密前寫入 Document Info / XMP metadata
    #[arg(long = "doc-author", value_name = "TEXT")]
    doc_author: Option<String>,

    /// 文件主旨；加密前寫入 Document Info / XMP metadata
    #[arg(long = "doc-subject", value_name = "TEXT")]
    doc_subject: Option<String>,

    /// 文件關鍵字；加密前寫入 Document Info / XMP metadata
    #[arg(long = "doc-keywords", value_name = "TEXT")]
    doc_keywords: Option<String>,

    /// 文件描述；加密前寫入 Document Info / XMP metadata
    #[arg(long = "doc-description", value_name = "TEXT")]
    doc_description: Option<String>,

    /// 文件著作權狀態；加密前寫入 Document Info / XMP metadata
    #[arg(long = "doc-copyright-status", value_name = "TEXT")]
    doc_copyright_status: Option<String>,

    /// 文件著作權聲明；加密前寫入 Document Info / XMP metadata
    #[arg(long = "doc-copyright-notice", value_name = "TEXT")]
    doc_copyright_notice: Option<String>,

    /// 文件著作權資訊 URL；加密前寫入 Document Info / XMP metadata
    #[arg(long = "doc-copyright-info-url", value_name = "URL")]
    doc_copyright_info_url: Option<String>,

    /// 不加密 PDF metadata stream
    #[arg(long = "no-encrypt-metadata")]
    no_encrypt_metadata: bool,
}

#[derive(Debug, Args)]
struct DecryptArgs {
    /// 輸入 PDF 檔案路徑
    #[arg(short, long, value_name = "FILE")]
    input: PathBuf,

    /// 輸出 PDF 檔案路徑 (預設為 input 解密字尾)
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// 使用者或擁有者密碼；未指定時使用空密碼
    #[arg(short = 'p', long, default_value = "", value_name = "PASSWORD")]
    password: String,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliAlgorithm {
    #[value(name = "aes-256")]
    Aes256,
    #[value(name = "aes-128")]
    Aes128,
    #[value(name = "rc4-128")]
    Rc4Bit128,
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
        Command::Encrypt(args) => run_encrypt(args),
        Command::Decrypt(args) => run_decrypt(args),
    };

    if let Err(err) = result {
        eprintln!("處理失敗：{err:#}");
        std::process::exit(1);
    }
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
    let report = encrypt_pdf_with_options(&args.input, &output_path, &options)?;

    println!("已完成加密：{}", output_path.display());
    print_report(&report);
    Ok(())
}

fn run_encrypt_in_place(args: &EncryptArgs, options: &EncryptOptions) -> anyhow::Result<()> {
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
    fn read_only_denies_modify_from_all_permissions() {
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
        assert!(options.permissions.contains(Permissions::ASSEMBLABLE));
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
}
