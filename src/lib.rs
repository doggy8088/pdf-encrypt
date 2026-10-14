//! Library component for the `pdf-encrypt` crate.
//!
//! The tool rewrites PDF files with lopdf's standard security handler support.
//! It can encrypt an unencrypted PDF with RC4/AES profiles or decrypt an
//! encrypted PDF when the correct user or owner password is provided.
//! It also reads and edits document properties while preserving existing encryption.

mod edit;
mod properties;
pub use edit::{edit_pdf_properties, EditOptions};
pub use properties::{read_pdf_properties, MetadataStatus, PdfProperties, PropertyConflict};

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use lopdf::encryption::crypt_filters::{Aes128CryptFilter, Aes256CryptFilter, CryptFilter};
use lopdf::{
    text_string, Dictionary, Document, EncryptionState, EncryptionVersion, Object, ObjectId,
    Permissions, StringFormat,
};

const FILE_ID_LEN: usize = 16;
const AES_256_KEY_LEN: usize = 32;
const STANDARD_CRYPT_FILTER: &[u8] = b"StdCF";

/// Default permissions for read-only mode.
///
/// This starts from all permissions and removes content modification and
/// document assembly. Callers can further tighten it by removing
/// print/copy/annotation permissions if needed.
pub fn read_only_permissions() -> Permissions {
    Permissions::all() & !Permissions::MODIFIABLE & !Permissions::ASSEMBLABLE
}

/// Supported PDF encryption algorithms.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EncryptionAlgorithm {
    /// PDF 2.0 AES-256 standard security handler.
    #[default]
    Aes256,
    /// PDF 1.5 AES-128 crypt filter.
    Aes128,
    /// PDF 1.4 RC4 with a 128-bit file encryption key.
    Rc4Bit128,
    /// PDF 1.4 RC4 with a 40-bit file encryption key.
    Rc4Bit40,
}

impl EncryptionAlgorithm {
    /// Human-readable CLI/report label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Aes256 => "aes-256",
            Self::Aes128 => "aes-128",
            Self::Rc4Bit128 => "rc4-128",
            Self::Rc4Bit40 => "rc4-40",
        }
    }

    fn minimum_pdf_version(self) -> &'static str {
        match self {
            Self::Aes256 => "2.0",
            Self::Aes128 => "1.5",
            Self::Rc4Bit128 | Self::Rc4Bit40 => "1.4",
        }
    }
}

/// Operation represented by a [`PdfCryptoReport`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfOperation {
    Encrypt,
    Decrypt,
    Edit,
}

impl PdfOperation {
    /// Human-readable CLI/report label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Encrypt => "encrypt",
            Self::Decrypt => "decrypt",
            Self::Edit => "edit",
        }
    }
}

/// Document information read from a PDF or written during editing/encryption.
/// `None` preserves a field; `Some("")` explicitly writes an empty value.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct DocumentInfo {
    /// Document title.
    pub title: Option<String>,
    /// Document author.
    pub author: Option<String>,
    /// Document subject.
    pub subject: Option<String>,
    /// Document keywords.
    pub keywords: Option<String>,
    /// Document description stored in XMP and as a custom Info field.
    pub description: Option<String>,
    /// Copyright status stored in XMP and as a custom Info field.
    pub copyright_status: Option<String>,
    /// Copyright notice stored in XMP and as a custom Info field.
    pub copyright_notice: Option<String>,
    /// Copyright information URL stored in XMP and as a custom Info field.
    pub copyright_info_url: Option<String>,
}

impl DocumentInfo {
    /// Returns true when no document information fields were provided.
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.author.is_none()
            && self.subject.is_none()
            && self.keywords.is_none()
            && self.description.is_none()
            && self.copyright_status.is_none()
            && self.copyright_notice.is_none()
            && self.copyright_info_url.is_none()
    }
}

/// Encryption settings used by [`encrypt_pdf_with_options`].
#[derive(Debug, Clone)]
pub struct EncryptOptions {
    /// Password required to open the encrypted output as a regular user.
    ///
    /// This corresponds to the CLI's output `--open-password` option.
    pub user_password: String,
    /// Owner password that can override permissions in compatible PDF viewers.
    pub owner_password: String,
    /// Encryption algorithm/profile.
    pub algorithm: EncryptionAlgorithm,
    /// PDF permission bits granted to users opening with the user password.
    pub permissions: Permissions,
    /// Whether XMP/document metadata streams should be encrypted.
    pub encrypt_metadata: bool,
    /// Optional password for opening an already-encrypted input before re-encrypting it.
    pub input_open_password: Option<String>,
    /// Optional document information to write before encrypting the output.
    pub document_info: DocumentInfo,
}

impl EncryptOptions {
    /// Create default encryption options with the same output open and owner password.
    pub fn with_open_password(password: impl Into<String>) -> Self {
        let password = password.into();
        Self {
            owner_password: password.clone(),
            user_password: password,
            ..Self::default()
        }
    }

    /// Create default encryption options with the same user and owner password.
    pub fn with_user_password(password: impl Into<String>) -> Self {
        Self::with_open_password(password)
    }

    /// Create encryption options for a read-only output open password.
    ///
    /// The owner password should be kept private and should differ from the
    /// open password if permission restrictions are expected to be meaningful
    /// in PDF readers.
    pub fn with_read_only_password(
        read_only_password: impl Into<String>,
        owner_password: impl Into<String>,
    ) -> Self {
        Self {
            user_password: read_only_password.into(),
            owner_password: owner_password.into(),
            permissions: read_only_permissions(),
            ..Self::default()
        }
    }
}

impl Default for EncryptOptions {
    fn default() -> Self {
        Self {
            user_password: String::new(),
            owner_password: String::new(),
            algorithm: EncryptionAlgorithm::default(),
            permissions: Permissions::all(),
            encrypt_metadata: true,
            input_open_password: None,
            document_info: DocumentInfo::default(),
        }
    }
}

/// Decryption settings used by [`decrypt_pdf_with_options`].
#[derive(Debug, Clone, Default)]
pub struct DecryptOptions {
    /// User or owner password for opening the encrypted input.
    pub password: String,
}

impl DecryptOptions {
    /// Create decryption options with the provided password.
    pub fn new(password: impl Into<String>) -> Self {
        Self {
            password: password.into(),
        }
    }
}

/// Summary of one encryption/decryption run.
#[derive(Debug, Clone)]
pub struct PdfCryptoReport {
    pub operation: PdfOperation,
    pub input_size: u64,
    pub output_size: u64,
    pub input_encrypted: bool,
    pub output_encrypted: bool,
    pub page_count: usize,
    pub object_count: usize,
    pub algorithm: Option<EncryptionAlgorithm>,
}

impl PdfCryptoReport {
    /// Positive values mean the output is larger than the input.
    pub fn size_delta(&self) -> i64 {
        self.output_size as i64 - self.input_size as i64
    }
}

/// Encrypt a PDF using the default AES-256 options and the same open/owner password.
pub fn encrypt_pdf<I, O>(input_path: I, output_path: O, user_password: &str) -> Result<()>
where
    I: AsRef<Path>,
    O: AsRef<Path>,
{
    let options = EncryptOptions::with_user_password(user_password);
    encrypt_pdf_with_options(input_path, output_path, &options).map(|_| ())
}

/// Encrypt a PDF and return a report describing the output.
pub fn encrypt_pdf_with_options<I, O>(
    input_path: I,
    output_path: O,
    options: &EncryptOptions,
) -> Result<PdfCryptoReport>
where
    I: AsRef<Path>,
    O: AsRef<Path>,
{
    let input_path = input_path.as_ref();
    let output_path = output_path.as_ref();
    let input_size = file_size(input_path, "input")?;
    let mut doc = load_document_for_encryption(input_path, options.input_open_password.as_deref())?;
    let input_encrypted = doc.was_encrypted();

    if doc.is_encrypted() {
        bail!(
            "Input PDF is encrypted; decrypt it first or provide --input-open-password: {}",
            input_path.display()
        );
    }

    apply_document_info(&mut doc, &options.document_info)?;
    ensure_file_identifier(&mut doc)?;
    ensure_minimum_pdf_version(&mut doc, options.algorithm.minimum_pdf_version());

    let page_count = doc.get_pages().len();
    let object_count = doc.objects.len();
    let mut file_encryption_key = [0u8; AES_256_KEY_LEN];
    let state = encryption_state_for(&doc, options, &mut file_encryption_key)?;

    doc.encrypt(&state)
        .context("Failed to encrypt PDF document")?;
    if options.algorithm == EncryptionAlgorithm::Aes256 {
        ensure_aes256_key_length_entry(&mut doc)?;
    }
    let output_size = write_document(&mut doc, output_path, "encrypted PDF file")?;

    Ok(PdfCryptoReport {
        operation: PdfOperation::Encrypt,
        input_size,
        output_size,
        input_encrypted,
        output_encrypted: true,
        page_count,
        object_count,
        algorithm: Some(options.algorithm),
    })
}

/// Decrypt a PDF using the provided user or owner password.
pub fn decrypt_pdf<I, O>(input_path: I, output_path: O, password: &str) -> Result<()>
where
    I: AsRef<Path>,
    O: AsRef<Path>,
{
    let options = DecryptOptions::new(password);
    decrypt_pdf_with_options(input_path, output_path, &options).map(|_| ())
}

/// Decrypt a PDF and return a report describing the output.
pub fn decrypt_pdf_with_options<I, O>(
    input_path: I,
    output_path: O,
    options: &DecryptOptions,
) -> Result<PdfCryptoReport>
where
    I: AsRef<Path>,
    O: AsRef<Path>,
{
    let input_path = input_path.as_ref();
    let output_path = output_path.as_ref();
    let input_size = file_size(input_path, "input")?;
    let mut doc = Document::load_with_password(input_path, &options.password)
        .with_context(|| format!("Failed to open PDF file: {}", input_path.display()))?;

    if doc.is_encrypted() {
        doc.decrypt(&options.password)
            .context("Failed to decrypt PDF document")?;
    }

    let input_encrypted = doc.was_encrypted();
    if !input_encrypted {
        bail!("Input PDF is not encrypted: {}", input_path.display());
    }

    let page_count = doc.get_pages().len();
    let object_count = doc.objects.len();
    let output_size = write_document(&mut doc, output_path, "decrypted PDF file")?;

    Ok(PdfCryptoReport {
        operation: PdfOperation::Decrypt,
        input_size,
        output_size,
        input_encrypted,
        output_encrypted: false,
        page_count,
        object_count,
        algorithm: None,
    })
}

fn load_document_for_encryption(input_path: &Path, password: Option<&str>) -> Result<Document> {
    match password {
        Some(password) => Document::load_with_password(input_path, password),
        None => Document::load(input_path),
    }
    .with_context(|| format!("Failed to open PDF file: {}", input_path.display()))
}

fn apply_document_info(doc: &mut Document, info: &DocumentInfo) -> Result<()> {
    if info.is_empty() {
        return Ok(());
    }

    update_info_dictionary(doc, info)?;
    edit::update_xmp_metadata(doc, info)?;
    Ok(())
}

fn update_info_dictionary(doc: &mut Document, info: &DocumentInfo) -> Result<()> {
    let info_id = ensure_info_dictionary_id(doc)?;
    let info_dict = doc
        .get_dictionary_mut(info_id)
        .context("Failed to access PDF Info dictionary")?;

    set_info_text(info_dict, b"Title", &info.title);
    set_info_text(info_dict, b"Author", &info.author);
    set_info_text(info_dict, b"Subject", &info.subject);
    set_info_text(info_dict, b"Keywords", &info.keywords);
    set_info_text(info_dict, b"Description", &info.description);
    set_info_text(info_dict, b"CopyrightStatus", &info.copyright_status);
    set_info_text(info_dict, b"CopyrightNotice", &info.copyright_notice);
    set_info_text(info_dict, b"CopyrightInfoURL", &info.copyright_info_url);
    Ok(())
}

fn ensure_info_dictionary_id(doc: &mut Document) -> Result<ObjectId> {
    let existing_info = doc.trailer.get(b"Info").cloned().ok();

    match existing_info {
        Some(Object::Reference(id)) => {
            doc.get_dictionary(id)
                .context("Invalid PDF Info dictionary")?;
            Ok(id)
        }
        Some(Object::Dictionary(dict)) => {
            let id = doc.add_object(dict);
            doc.trailer.set("Info", id);
            Ok(id)
        }
        None | Some(Object::Null) => {
            let id = doc.add_object(Dictionary::new());
            doc.trailer.set("Info", id);
            Ok(id)
        }
        _ => bail!("Invalid PDF Info dictionary; refusing to discard existing metadata"),
    }
}

fn set_info_text(info_dict: &mut Dictionary, key: &[u8], value: &Option<String>) {
    if let Some(value) = value {
        info_dict.set(key, text_string(value));
    }
}

fn build_xmp_metadata(info: &DocumentInfo) -> String {
    let mut fields = String::new();

    if let Some(value) = &info.title {
        push_xmp_alt_text(&mut fields, "dc:title", value);
    }
    if let Some(value) = &info.author {
        push_xmp_sequence(&mut fields, "dc:creator", value);
    }
    if let Some(value) = &info.subject {
        push_xmp_simple(&mut fields, "pdfencrypt:Subject", value);
    }
    if let Some(value) = &info.keywords {
        push_xmp_simple(&mut fields, "pdf:Keywords", value);
        push_xmp_keywords(&mut fields, value);
    }
    if let Some(value) = &info.description {
        push_xmp_alt_text(&mut fields, "dc:description", value);
    }
    if let Some(value) = &info.copyright_status {
        if let Some(marked) = copyright_status_to_xmp_marked(value) {
            push_xmp_simple(&mut fields, "xmpRights:Marked", marked);
        }
        push_xmp_simple(&mut fields, "pdfencrypt:CopyrightStatus", value);
    }
    if let Some(value) = &info.copyright_notice {
        push_xmp_alt_text(&mut fields, "dc:rights", value);
    }
    if let Some(value) = &info.copyright_info_url {
        push_xmp_simple(&mut fields, "xmpRights:WebStatement", value);
    }

    format!(
        r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description rdf:about=""
        xmlns:dc="http://purl.org/dc/elements/1.1/"
        xmlns:pdf="http://ns.adobe.com/pdf/1.3/"
        xmlns:xmpRights="http://ns.adobe.com/xap/1.0/rights/"
        xmlns:pdfencrypt="https://github.com/doggy8088/pdf-encrypt/ns/1.0/">
{fields}    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>
"#
    )
}

fn push_xmp_simple(fields: &mut String, tag: &str, value: &str) {
    fields.push_str("      <");
    fields.push_str(tag);
    fields.push('>');
    fields.push_str(&escape_xml(value));
    fields.push_str("</");
    fields.push_str(tag);
    fields.push_str(">\n");
}

fn push_xmp_alt_text(fields: &mut String, tag: &str, value: &str) {
    fields.push_str("      <");
    fields.push_str(tag);
    fields.push_str("><rdf:Alt><rdf:li xml:lang=\"x-default\">");
    fields.push_str(&escape_xml(value));
    fields.push_str("</rdf:li></rdf:Alt></");
    fields.push_str(tag);
    fields.push_str(">\n");
}

fn push_xmp_sequence(fields: &mut String, tag: &str, value: &str) {
    fields.push_str("      <");
    fields.push_str(tag);
    fields.push_str("><rdf:Seq><rdf:li>");
    fields.push_str(&escape_xml(value));
    fields.push_str("</rdf:li></rdf:Seq></");
    fields.push_str(tag);
    fields.push_str(">\n");
}

fn push_xmp_keywords(fields: &mut String, keywords: &str) {
    let items: Vec<_> = keywords
        .split([',', ';'])
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .collect();

    if items.is_empty() {
        return;
    }

    fields.push_str("      <dc:subject><rdf:Bag>");
    for item in items {
        fields.push_str("<rdf:li>");
        fields.push_str(&escape_xml(item));
        fields.push_str("</rdf:li>");
    }
    fields.push_str("</rdf:Bag></dc:subject>\n");
}

fn copyright_status_to_xmp_marked(status: &str) -> Option<&'static str> {
    match status.trim().to_ascii_lowercase().as_str() {
        "copyrighted" | "copyright" | "true" | "yes" | "marked" | "1" => Some("True"),
        "public-domain" | "public domain" | "publicdomain" | "false" | "no" | "unmarked" | "0" => {
            Some("False")
        }
        _ => None,
    }
}

fn escape_xml(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn encryption_state_for(
    doc: &Document,
    options: &EncryptOptions,
    file_encryption_key: &mut [u8; AES_256_KEY_LEN],
) -> Result<EncryptionState> {
    let owner_password = options.owner_password.as_str();
    let user_password = options.user_password.as_str();
    let permissions = options.permissions;

    let requested_version = match options.algorithm {
        EncryptionAlgorithm::Aes256 => {
            getrandom::fill(file_encryption_key)
                .context("Failed to generate AES-256 file encryption key")?;
            let crypt_filter: Arc<dyn CryptFilter> = Arc::new(Aes256CryptFilter);
            EncryptionVersion::V5 {
                encrypt_metadata: options.encrypt_metadata,
                crypt_filters: standard_crypt_filters(crypt_filter),
                file_encryption_key,
                stream_filter: STANDARD_CRYPT_FILTER.to_vec(),
                string_filter: STANDARD_CRYPT_FILTER.to_vec(),
                owner_password,
                user_password,
                permissions,
            }
        }
        EncryptionAlgorithm::Aes128 => {
            let crypt_filter: Arc<dyn CryptFilter> = Arc::new(Aes128CryptFilter);
            EncryptionVersion::V4 {
                document: doc,
                encrypt_metadata: options.encrypt_metadata,
                crypt_filters: standard_crypt_filters(crypt_filter),
                stream_filter: STANDARD_CRYPT_FILTER.to_vec(),
                string_filter: STANDARD_CRYPT_FILTER.to_vec(),
                owner_password,
                user_password,
                permissions,
            }
        }
        EncryptionAlgorithm::Rc4Bit128 => EncryptionVersion::V2 {
            document: doc,
            owner_password,
            user_password,
            key_length: 128,
            permissions,
        },
        EncryptionAlgorithm::Rc4Bit40 => EncryptionVersion::V1 {
            document: doc,
            owner_password,
            user_password,
            permissions,
        },
    };

    EncryptionState::try_from(requested_version).context("Failed to create PDF encryption state")
}

fn standard_crypt_filters(
    crypt_filter: Arc<dyn CryptFilter>,
) -> BTreeMap<Vec<u8>, Arc<dyn CryptFilter>> {
    BTreeMap::from([(STANDARD_CRYPT_FILTER.to_vec(), crypt_filter)])
}

fn ensure_file_identifier(doc: &mut Document) -> Result<()> {
    if doc.trailer.get(b"ID").is_ok() {
        return Ok(());
    }

    let mut id = [0u8; FILE_ID_LEN];
    getrandom::fill(&mut id).context("Failed to generate PDF file identifier")?;
    let id_object = Object::String(id.to_vec(), StringFormat::Hexadecimal);
    doc.trailer
        .set(b"ID", Object::Array(vec![id_object.clone(), id_object]));
    Ok(())
}

fn ensure_minimum_pdf_version(doc: &mut Document, minimum: &str) {
    if compare_pdf_versions(&doc.version, minimum) == Ordering::Less {
        doc.version = minimum.to_string();
    }
}

fn ensure_aes256_key_length_entry(doc: &mut Document) -> Result<()> {
    let encrypt_id = doc
        .trailer
        .get(b"Encrypt")
        .and_then(Object::as_reference)
        .context("Encrypted PDF trailer does not reference an encryption dictionary")?;
    let encrypt_dict = doc
        .get_dictionary_mut(encrypt_id)
        .context("Failed to access encryption dictionary")?;
    encrypt_dict.set(b"Length", Object::Integer(256));

    // Some PDF readers, including macOS Preview/PDFKit, require the AESV3
    // crypt filter dictionary to include the key length in bytes. Without this
    // entry the file can unlock, but page contents render blank.
    let crypt_filter = encrypt_dict
        .get_mut(b"CF")
        .and_then(Object::as_dict_mut)
        .and_then(|filters| filters.get_mut(STANDARD_CRYPT_FILTER))
        .and_then(Object::as_dict_mut)
        .context("Failed to access standard AES-256 crypt filter dictionary")?;
    crypt_filter.set(b"Length", Object::Integer(32));
    Ok(())
}

fn compare_pdf_versions(left: &str, right: &str) -> Ordering {
    parse_pdf_version(left).cmp(&parse_pdf_version(right))
}

fn parse_pdf_version(version: &str) -> (u32, u32) {
    let mut parts = version.split('.');
    let major = parts.next().and_then(|part| part.parse().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|part| part.parse().ok()).unwrap_or(0);
    (major, minor)
}

fn write_document(doc: &mut Document, output_path: &Path, label: &str) -> Result<u64> {
    let mut output = File::create(output_path)
        .with_context(|| format!("Failed to create output file: {}", output_path.display()))?;
    doc.save_to(&mut output)
        .with_context(|| format!("Failed to write {label}"))?;
    output.flush()?;
    file_size(output_path, "output")
}

fn file_size(path: &Path, label: &str) -> Result<u64> {
    Ok(fs::metadata(path)
        .with_context(|| format!("Failed to read {label} metadata: {}", path.display()))?
        .len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_pdf_versions_numerically() {
        assert_eq!(compare_pdf_versions("1.7", "2.0"), Ordering::Less);
        assert_eq!(compare_pdf_versions("1.10", "1.9"), Ordering::Greater);
        assert_eq!(compare_pdf_versions("2.0", "2.0"), Ordering::Equal);
    }

    #[test]
    fn default_algorithm_is_aes256() {
        assert_eq!(EncryptionAlgorithm::default(), EncryptionAlgorithm::Aes256);
    }

    #[test]
    fn read_only_permissions_allow_all_except_modify_and_assemble() {
        let permissions = read_only_permissions();

        assert!(permissions.contains(Permissions::PRINTABLE));
        assert!(permissions.contains(Permissions::COPYABLE));
        assert!(permissions.contains(Permissions::ANNOTABLE));
        assert!(permissions.contains(Permissions::FILLABLE));
        assert!(!permissions.contains(Permissions::ASSEMBLABLE));
        assert!(!permissions.contains(Permissions::MODIFIABLE));
    }
}
