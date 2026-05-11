//! Integration tests for pdf-encrypt.
//!
//! These tests build small PDFs with lopdf and verify encryption, decryption,
//! password handling, and non-encrypted input validation.

use std::fs;
use std::path::Path;

use lopdf::{dictionary, Document, Object, Permissions, Stream};
use pdf_encrypt::{
    decrypt_pdf, decrypt_pdf_with_options, encrypt_pdf, encrypt_pdf_with_options, DecryptOptions,
    EncryptOptions, EncryptionAlgorithm, PdfOperation,
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
fn read_only_password_disables_edit_permissions() {
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
    assert!(!encrypted_permissions.contains(Permissions::MODIFIABLE));
    assert!(!encrypted_permissions.contains(Permissions::ANNOTABLE));
    assert!(!encrypted_permissions.contains(Permissions::FILLABLE));
    assert!(!encrypted_permissions.contains(Permissions::ASSEMBLABLE));

    let opened_with_viewer = Document::load_with_password(&output_path, "viewer")
        .expect("read-only password should open PDF");
    assert_eq!(opened_with_viewer.get_pages().len(), 1);
    let opened_with_owner = Document::load_with_password(&output_path, "owner")
        .expect("owner password should open PDF");
    assert_eq!(opened_with_owner.get_pages().len(), 1);
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
