//! Command line interface for the `pdf-encrypt` tool.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use clap::{ArgGroup, Args, Parser, Subcommand, ValueEnum};
use lopdf::Permissions;
use pdf_encrypt::{
    decrypt_pdf_with_options, encrypt_pdf_with_options, DecryptOptions, EncryptOptions,
    EncryptionAlgorithm, PdfCryptoReport,
};

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
    Encrypt(EncryptArgs),
    /// 將已加密 PDF 解密並寫出新檔案
    Decrypt(DecryptArgs),
}

#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("encrypt_password")
        .required(true)
        .args(["user_password", "read_only_password"])
))]
struct EncryptArgs {
    /// 輸入 PDF 檔案路徑
    #[arg(short, long, value_name = "FILE")]
    input: PathBuf,

    /// 輸出 PDF 檔案路徑 (預設為 input 加密字尾)
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// 開啟加密輸出檔所需的使用者密碼；預設允許全部權限
    #[arg(
        short = 'p',
        long,
        value_name = "PASSWORD",
        conflicts_with = "read_only_password"
    )]
    user_password: Option<String>,

    /// 讀取專用密碼；預設禁止修改、註解、填表與重組頁面
    #[arg(
        long,
        value_name = "PASSWORD",
        conflicts_with = "user_password",
        requires = "owner_password"
    )]
    read_only_password: Option<String>,

    /// 擁有者密碼；使用 read-only password 時必填且不可相同
    #[arg(long, value_name = "PASSWORD")]
    owner_password: Option<String>,

    /// 輸入檔若已加密，用此密碼先開啟再重新加密
    #[arg(long, value_name = "PASSWORD")]
    open_password: Option<String>,

    /// 加密演算法
    #[arg(long, value_enum, default_value_t = CliAlgorithm::Aes256)]
    algorithm: CliAlgorithm,

    /// 只允許指定權限；可重複。未指定時預設允許全部權限
    #[arg(long = "allow", value_enum, value_name = "PERMISSION")]
    allow: Vec<CliPermission>,

    /// 從目前允許權限中移除指定權限；可重複
    #[arg(long = "deny", value_enum, value_name = "PERMISSION")]
    deny: Vec<CliPermission>,

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
    let cli = Cli::parse();

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
    let output_path = args
        .output
        .clone()
        .unwrap_or_else(|| default_output_path(&args.input, "encrypted"));
    let options = encrypt_options_from_args(args)?;
    let report = encrypt_pdf_with_options(&args.input, &output_path, &options)?;

    println!("已完成加密：{}", output_path.display());
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

fn encrypt_options_from_args(args: &EncryptArgs) -> anyhow::Result<EncryptOptions> {
    let mut options = if let Some(read_only_password) = &args.read_only_password {
        let owner_password = args
            .owner_password
            .clone()
            .context("--read-only-password requires --owner-password")?;
        if owner_password == *read_only_password {
            bail!("--owner-password must be different from --read-only-password");
        }
        EncryptOptions::with_read_only_password(read_only_password.clone(), owner_password)
    } else {
        let user_password = args
            .user_password
            .clone()
            .context("encrypt requires --user-password or --read-only-password")?;
        let mut options = EncryptOptions::with_user_password(user_password.clone());
        options.owner_password = args.owner_password.clone().unwrap_or(user_password);
        options
    };

    options.open_password = args.open_password.clone();
    options.algorithm = args.algorithm.into();
    options.permissions = permissions_from_args(&args.allow, &args.deny, options.permissions);
    options.encrypt_metadata = !args.no_encrypt_metadata;
    Ok(options)
}

fn permissions_from_args(
    allow: &[CliPermission],
    deny: &[CliPermission],
    default_permissions: Permissions,
) -> Permissions {
    let mut permissions = if allow.is_empty() {
        default_permissions
    } else {
        allow
            .iter()
            .fold(Permissions::empty(), |acc, item| acc | item.permissions())
    };

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
    fn allow_list_replaces_default_permissions() {
        let permissions = permissions_from_args(
            &[CliPermission::Print, CliPermission::Copy],
            &[],
            Permissions::all(),
        );

        assert!(permissions.contains(Permissions::PRINTABLE));
        assert!(permissions.contains(Permissions::COPYABLE));
        assert!(!permissions.contains(Permissions::MODIFIABLE));
    }

    #[test]
    fn deny_list_removes_permissions_from_default() {
        let permissions = permissions_from_args(&[], &[CliPermission::Copy], Permissions::all());

        assert!(!permissions.contains(Permissions::COPYABLE));
        assert!(permissions.contains(Permissions::PRINTABLE));
    }

    #[test]
    fn read_only_password_sets_restricted_user_password() {
        let cli = Cli::try_parse_from([
            "pdf-encrypt",
            "encrypt",
            "--input",
            "input.pdf",
            "--read-only-password",
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
        assert_eq!(options.permissions, pdf_encrypt::read_only_permissions());
    }

    #[test]
    fn read_only_password_requires_owner_password() {
        let result = Cli::try_parse_from([
            "pdf-encrypt",
            "encrypt",
            "--input",
            "input.pdf",
            "--read-only-password",
            "viewer",
        ]);

        assert!(result.is_err());
    }

    #[test]
    fn read_only_password_must_differ_from_owner_password() {
        let cli = Cli::try_parse_from([
            "pdf-encrypt",
            "encrypt",
            "--input",
            "input.pdf",
            "--read-only-password",
            "same",
            "--owner-password",
            "same",
        ])
        .unwrap();
        let Command::Encrypt(args) = cli.command else {
            panic!("expected encrypt command");
        };

        assert!(encrypt_options_from_args(&args).is_err());
    }
}
