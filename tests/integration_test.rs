//! Integration tests for pdf-encrypt.
//!
//! These tests build small PDFs with lopdf and verify encryption, decryption,
//! password handling, and non-encrypted input validation.

use std::fs;
use std::path::Path;
use std::process::Command as ProcessCommand;

use lopdf::{decode_text_string, dictionary, Document, Object, Permissions, Stream};
use pdf_encrypt::{
    decrypt_pdf, decrypt_pdf_with_options, encrypt_pdf, encrypt_pdf_with_options, DecryptOptions,
    DocumentInfo, EncryptOptions, EncryptionAlgorithm, PdfOperation,
};

fn make_simple_pdf(path: &Path) {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! {
            "F1" => font_id,
        },
    });
    let content = lopdf::content::Content {
        operations: vec![
            lopdf::content::Operation::new("BT", vec![]),
            lopdf::content::Operation::new("Tf", vec!["F1".into(), 48.into()]),
            lopdf::content::Operation::new("Td", vec![100.into(), 700.into()]),
            lopdf::content::Operation::new("Tj", vec![Object::string_literal("Hello PDF!")]),
            lopdf::content::Operation::new("ET", vec![]),
        ],
    };
    let content_stream = Stream::new(
        lopdf::dictionary! {},
        content.encode().expect("content encodes"),
    );
    let content_id = doc.add_object(content_stream);
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
    });
    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![page_id.into()],
        "Count" => 1,
        "Resources" => resources_id,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc.save(path).expect("test PDF saves");
}

fn document_info_text(doc: &Document, key: &[u8]) -> String {
    let info_id = doc
        .trailer
        .get(b"Info")
        .and_then(Object::as_reference)
        .expect("PDF should have Info dictionary");
    let info_dict = doc
        .get_dictionary(info_id)
        .expect("Info object should be a dictionary");
    decode_text_string(info_dict.get(key).expect("Info field should exist"))
        .expect("Info field should decode as text")
}

#[test]
fn encrypts_pdf_with_default_aes256_and_requires_password() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input_path = dir.path().join("plain.pdf");
    let output_path = dir.path().join("encrypted.pdf");
    make_simple_pdf(&input_path);

    let report = encrypt_pdf_with_options(
        &input_path,
        &output_path,
        &EncryptOptions::with_user_password("secret"),
    )
    .expect("encryption should succeed");

    assert_eq!(report.operation, PdfOperation::Encrypt);
    assert_eq!(report.algorithm, Some(EncryptionAlgorithm::Aes256));
    assert!(!report.input_encrypted);
    assert!(report.output_encrypted);
    assert_eq!(report.page_count, 1);
    assert!(output_path.exists());

    let locked =
        Document::load(&output_path).expect("encrypted PDF should be loadable as metadata");
    assert!(locked.is_encrypted());
    let encrypt_dict = locked
        .get_encrypted()
        .expect("encrypted PDF exposes encryption dictionary");
    assert_eq!(
        encrypt_dict
            .get(b"Length")
            .and_then(Object::as_i64)
            .expect("AES-256 encryption dictionary has key length"),
        256
    );
    assert_eq!(
        encrypt_dict
            .get(b"CF")
            .and_then(Object::as_dict)
            .and_then(|filters| filters.get(b"StdCF"))
            .and_then(Object::as_dict)
            .and_then(|filter| filter.get(b"Length"))
            .and_then(Object::as_i64)
            .expect("AES-256 crypt filter has key length"),
        32
    );

    let unlocked = Document::load_with_password(&output_path, "secret")
        .expect("encrypted PDF should open with password");
    assert!(unlocked.was_encrypted());
    assert!(!unlocked.is_encrypted());
    assert_eq!(unlocked.get_pages().len(), 1);
}

#[test]
fn decrypts_encrypted_pdf() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input_path = dir.path().join("plain.pdf");
    let encrypted_path = dir.path().join("encrypted.pdf");
    let decrypted_path = dir.path().join("decrypted.pdf");
    make_simple_pdf(&input_path);
    encrypt_pdf(&input_path, &encrypted_path, "secret").expect("encryption should succeed");

    let report = decrypt_pdf_with_options(
        &encrypted_path,
        &decrypted_path,
        &DecryptOptions::new("secret"),
    )
    .expect("decryption should succeed");

    assert_eq!(report.operation, PdfOperation::Decrypt);
    assert!(report.input_encrypted);
    assert!(!report.output_encrypted);
    assert_eq!(report.page_count, 1);
    assert!(decrypted_path.exists());

    let decrypted = Document::load(&decrypted_path).expect("decrypted PDF should be readable");
    assert!(!decrypted.is_encrypted());
    assert_eq!(decrypted.get_pages().len(), 1);
}

#[test]
fn encrypts_with_aes128_and_restricted_permissions() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input_path = dir.path().join("plain.pdf");
    let output_path = dir.path().join("encrypted_aes128.pdf");
    make_simple_pdf(&input_path);

    let mut options = EncryptOptions::with_user_password("secret");
    options.algorithm = EncryptionAlgorithm::Aes128;
    options.permissions = Permissions::PRINTABLE | Permissions::COPYABLE;

    let report = encrypt_pdf_with_options(&input_path, &output_path, &options)
        .expect("AES-128 encryption should succeed");

    assert_eq!(report.algorithm, Some(EncryptionAlgorithm::Aes128));
    assert!(output_path.exists());

    let unlocked = Document::load_with_password(&output_path, "secret")
        .expect("AES-128 encrypted PDF should open with password");
    assert!(unlocked.was_encrypted());
    assert_eq!(unlocked.get_pages().len(), 1);
}

#[test]
fn cli_encrypt_in_place_overwrites_input_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input_path = dir.path().join("plain.pdf");
    make_simple_pdf(&input_path);

    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_pdf-encrypt"))
        .arg("encrypt")
        .arg("--input")
        .arg(&input_path)
        .arg("--owner-password")
        .arg("owner")
        .arg("--read-only")
        .arg("--in-place")
        .arg("--force")
        .output()
        .expect("CLI should run");

    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!dir.path().join("plain_encrypted.pdf").exists());

    let locked = Document::load(&input_path).expect("in-place encrypted PDF should load");
    assert!(locked.was_encrypted());

    let opened_without_password = Document::load_with_password(&input_path, "")
        .expect("in-place encrypted PDF should open without password");
    assert!(opened_without_password.was_encrypted());
    assert_eq!(opened_without_password.get_pages().len(), 1);

    let opened_with_owner = Document::load_with_password(&input_path, "owner")
        .expect("owner password should open in-place encrypted PDF");
    assert!(opened_with_owner.was_encrypted());
    assert_eq!(opened_with_owner.get_pages().len(), 1);
}

#[test]
fn read_only_password_allows_all_except_modify() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input_path = dir.path().join("plain.pdf");
    let output_path = dir.path().join("readonly.pdf");
    make_simple_pdf(&input_path);

    let options = EncryptOptions::with_read_only_password("viewer", "owner");
    let report = encrypt_pdf_with_options(&input_path, &output_path, &options)
        .expect("read-only encryption should succeed");

    assert_eq!(report.algorithm, Some(EncryptionAlgorithm::Aes256));
    let locked = Document::load(&output_path).expect("read-only encrypted PDF should load");
    let encrypted_permissions = locked
        .get_encrypted()
        .and_then(|dict| dict.get(b"P"))
        .and_then(Object::as_i64)
        .map(|value| Permissions::from_bits_retain(value as u64))
        .expect("encrypted PDF should expose permissions");
    assert!(encrypted_permissions.contains(Permissions::PRINTABLE));
    assert!(encrypted_permissions.contains(Permissions::COPYABLE));
    assert!(encrypted_permissions.contains(Permissions::ANNOTABLE));
    assert!(encrypted_permissions.contains(Permissions::FILLABLE));
    assert!(encrypted_permissions.contains(Permissions::ASSEMBLABLE));
    assert!(!encrypted_permissions.contains(Permissions::MODIFIABLE));

    let opened_with_viewer = Document::load_with_password(&output_path, "viewer")
        .expect("read-only password should open PDF");
    assert_eq!(opened_with_viewer.get_pages().len(), 1);
    let opened_with_owner = Document::load_with_password(&output_path, "owner")
        .expect("owner password should open PDF");
    assert_eq!(opened_with_owner.get_pages().len(), 1);
}

#[test]
fn encrypts_after_updating_document_info() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input_path = dir.path().join("plain.pdf");
    let output_path = dir.path().join("metadata.pdf");
    make_simple_pdf(&input_path);

    let mut options = EncryptOptions::with_open_password("secret");
    options.document_info = DocumentInfo {
        title: Some("規格驅動開發實戰".to_string()),
        author: Some("Will 保哥".to_string()),
        subject: Some("AI 時代的軟體開發新典範".to_string()),
        keywords: Some("AI, SDD, Rust".to_string()),
        description: Some("課程講義".to_string()),
        copyright_status: Some("Copyrighted".to_string()),
        copyright_notice: Some("Copyright © Will 保哥".to_string()),
        copyright_info_url: Some("https://example.com/copyright".to_string()),
    };

    encrypt_pdf_with_options(&input_path, &output_path, &options)
        .expect("metadata encryption should succeed");

    let unlocked = Document::load_with_password(&output_path, "secret")
        .expect("encrypted PDF should open with password");
    assert_eq!(document_info_text(&unlocked, b"Title"), "規格驅動開發實戰");
    assert_eq!(document_info_text(&unlocked, b"Author"), "Will 保哥");
    assert_eq!(
        document_info_text(&unlocked, b"Subject"),
        "AI 時代的軟體開發新典範"
    );
    assert_eq!(document_info_text(&unlocked, b"Keywords"), "AI, SDD, Rust");
    assert_eq!(document_info_text(&unlocked, b"Description"), "課程講義");
    assert_eq!(
        document_info_text(&unlocked, b"CopyrightStatus"),
        "Copyrighted"
    );
    assert_eq!(
        document_info_text(&unlocked, b"CopyrightNotice"),
        "Copyright © Will 保哥"
    );
    assert_eq!(
        document_info_text(&unlocked, b"CopyrightInfoURL"),
        "https://example.com/copyright"
    );

    let catalog_id = unlocked
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .expect("PDF should have catalog");
    let metadata_id = unlocked
        .get_dictionary(catalog_id)
        .expect("catalog should be a dictionary")
        .get(b"Metadata")
        .and_then(Object::as_reference)
        .expect("catalog should reference XMP metadata");
    let metadata_stream = unlocked
        .get_object(metadata_id)
        .and_then(Object::as_stream)
        .expect("metadata should be a stream");
    let xmp = String::from_utf8_lossy(&metadata_stream.content);
    assert!(xmp.contains("<dc:title>"));
    assert!(xmp.contains("規格驅動開發實戰"));
    assert!(xmp.contains("<xmpRights:Marked>True</xmpRights:Marked>"));
    assert!(xmp.contains("https://example.com/copyright"));
}

#[test]
fn decrypting_plain_pdf_returns_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input_path = dir.path().join("plain.pdf");
    let output_path = dir.path().join("plain_decrypted.pdf");
    make_simple_pdf(&input_path);

    let result = decrypt_pdf(&input_path, &output_path, "");

    assert!(result.is_err());
    assert!(!output_path.exists());
}

#[test]
fn wrong_password_returns_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input_path = dir.path().join("plain.pdf");
    let encrypted_path = dir.path().join("encrypted.pdf");
    let decrypted_path = dir.path().join("decrypted.pdf");
    make_simple_pdf(&input_path);
    encrypt_pdf(&input_path, &encrypted_path, "secret").expect("encryption should succeed");

    let result = decrypt_pdf(&encrypted_path, &decrypted_path, "wrong");

    assert!(result.is_err());
    assert!(!decrypted_path.exists());
    assert!(
        fs::metadata(&encrypted_path)
            .expect("encrypted metadata")
            .len()
            > 0
    );
}
