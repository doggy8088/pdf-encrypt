use std::fs;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use lopdf::{dictionary, text_string, Document, Object, Stream};
use pdf_encrypt::{
    encrypt_pdf_with_options, read_pdf_properties, DocumentInfo, EncryptOptions,
    EncryptionAlgorithm,
};

const XMP: &str = r#"<?xpacket begin="" id="test"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/"><r:RDF xmlns:r="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<r:Description r:about="uuid:document" xmlns:d="http://purl.org/dc/elements/1.1/"
 xmlns:p="http://ns.adobe.com/pdf/1.3/" xmlns:rights="http://ns.adobe.com/xap/1.0/rights/"
 xmlns:custom="https://example.com/custom/" p:Keywords="Old keywords" custom:Attribute="keep attribute">
 <d:title><r:Alt><r:li xml:lang="x-default">Old title</r:li></r:Alt></d:title>
 <d:creator><r:Seq><r:li>Original author</r:li></r:Seq></d:creator>
 <d:description><r:Alt><r:li xml:lang="x-default">Original description</r:li></r:Alt></d:description>
 <d:subject><r:Bag><r:li>Old keywords</r:li></r:Bag></d:subject>
 <rights:Marked>True</rights:Marked>
 <custom:Unrelated><r:Description><d:title>Nested title</d:title><custom:Value>keep nested value</custom:Value></r:Description></custom:Unrelated>
</r:Description></r:RDF></x:xmpmeta><?xpacket end="w"?>"#;

fn make_pdf(path: &Path, xmp: Option<&str>) {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let content_id = doc.add_object(Stream::new(dictionary! {}, b"q 1 0 0 1 0 0 cm Q".to_vec()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "Contents" => content_id,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
        }),
    );
    let mut catalog = dictionary! { "Type" => "Catalog", "Pages" => pages_id };
    if let Some(xmp) = xmp {
        let mut stream = Stream::new(
            dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
            xmp.as_bytes().to_vec(),
        );
        stream.compress().unwrap();
        catalog.set("Metadata", doc.add_object(stream));
    }
    let catalog_id = doc.add_object(catalog);
    doc.trailer.set("Root", catalog_id);
    let info_id = doc.add_object(dictionary! {
        "Title" => text_string("Old title"), "Author" => text_string("Original author"),
        "Subject" => text_string("Original subject"), "Description" => text_string("Original description"),
        "Keywords" => text_string("Old keywords"), "Creator" => text_string("Original creator"),
        "Producer" => text_string("Original producer"), "CreationDate" => text_string("D:20261015120000+08'00'"),
        "CustomField" => text_string("keep custom field"),
    });
    doc.trailer.set("Info", info_id);
    doc.save(path).unwrap();
}

fn edit(input: &Path, output: Option<&Path>, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pdf-encrypt"));
    command.args(["edit", "-i"]).arg(input);
    if let Some(output) = output {
        command.arg("-o").arg(output);
    }
    command.args(args).stdin(Stdio::null()).output().unwrap()
}

fn assert_success(output: Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("operation: edit"));
}

fn metadata(doc: &Document) -> Vec<u8> {
    doc.catalog()
        .unwrap()
        .get_deref(b"Metadata", doc)
        .unwrap()
        .as_stream()
        .unwrap()
        .get_plain_content()
        .unwrap()
}

#[test]
fn edits_only_selected_fields_and_preserves_original_content_and_other_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.pdf");
    let output = dir.path().join("edited.pdf");
    make_pdf(&input, Some(XMP));
    let original_bytes = fs::read(&input).unwrap();
    let original = Document::load(&input).unwrap();
    assert_success(edit(
        &input,
        Some(&output),
        &["--doc-title", "新標題 & <測試>"],
    ));
    assert_eq!(fs::read(&input).unwrap(), original_bytes);
    let props = read_pdf_properties(&output, None).unwrap();
    assert_eq!(
        props.document_info.title.as_deref(),
        Some("新標題 & <測試>")
    );
    assert_eq!(
        props.document_info.author.as_deref(),
        Some("Original author")
    );
    assert_eq!(
        props.document_info.subject.as_deref(),
        Some("Original subject")
    );
    assert_eq!(
        props.document_info.keywords.as_deref(),
        Some("Old keywords")
    );
    assert_eq!(props.info_dictionary["Creator"], "Original creator");
    assert_eq!(props.info_dictionary["Producer"], "Original producer");
    assert_eq!(props.info_dictionary["CustomField"], "keep custom field");
    assert_eq!(
        props.xmp_properties["{https://example.com/custom/}Attribute"],
        vec!["keep attribute"]
    );
    let edited = Document::load(&output).unwrap();
    assert!(!edited.was_encrypted());
    let before_page = *original.get_pages().values().next().unwrap();
    let after_page = *edited.get_pages().values().next().unwrap();
    assert_eq!(
        original.get_page_content(before_page).unwrap(),
        edited.get_page_content(after_page).unwrap()
    );
    let xmp_bytes = metadata(&edited);
    let xmp = std::str::from_utf8(&xmp_bytes).unwrap();
    assert!(xmp.contains("<d:title>Nested title</d:title>"));
    assert!(xmp.contains("keep nested value"));
    assert!(!xmp.contains(">Old title<"));
    roxmltree::Document::parse(xmp).unwrap();
}

#[test]
fn edits_all_document_fields_and_clears_keywords_and_copyright_status() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.pdf");
    let output = dir.path().join("edited.pdf");
    make_pdf(&input, Some(XMP));
    assert_success(edit(
        &input,
        Some(&output),
        &[
            "--doc-title",
            "標題",
            "--doc-author",
            "作者",
            "--doc-subject",
            "主旨",
            "--doc-description",
            "描述 & <xml> \"quoted\"",
            "--doc-keywords",
            "",
            "--doc-copyright-status",
            "",
            "--doc-copyright-notice",
            "版權 © 2026",
            "--doc-copyright-info-url",
            "https://example.com/copyright?a=1&b=2",
        ],
    ));
    let props = read_pdf_properties(&output, None).unwrap();
    assert_eq!(
        props.document_info,
        DocumentInfo {
            title: Some("標題".into()),
            author: Some("作者".into()),
            subject: Some("主旨".into()),
            keywords: Some("".into()),
            description: Some("描述 & <xml> \"quoted\"".into()),
            copyright_status: Some("".into()),
            copyright_notice: Some("版權 © 2026".into()),
            copyright_info_url: Some("https://example.com/copyright?a=1&b=2".into()),
        }
    );
    assert!(props.conflicts.is_empty(), "{:?}", props.conflicts);
    assert!(!props
        .xmp_properties
        .contains_key("{http://ns.adobe.com/xap/1.0/rights/}Marked"));
    assert!(!props
        .xmp_properties
        .contains_key("{http://purl.org/dc/elements/1.1/}subject"));
}

#[test]
fn creates_missing_xmp_and_expands_a_self_closing_rdf_root() {
    let dir = tempfile::tempdir().unwrap();
    for xmp in [
        None,
        Some(r#"<r:RDF xmlns:r="http://www.w3.org/1999/02/22-rdf-syntax-ns#"/>"#),
    ] {
        let input = dir.path().join("input.pdf");
        let output = dir.path().join("edited.pdf");
        make_pdf(&input, xmp);
        assert_success(edit(
            &input,
            Some(&output),
            &["--force", "--doc-title", "新標題"],
        ));
        let props = read_pdf_properties(&output, None).unwrap();
        assert_eq!(props.document_info.title.as_deref(), Some("新標題"));
        assert_eq!(props.xmp_status, pdf_encrypt::MetadataStatus::Read);
        assert_eq!(props.info_dictionary["CustomField"], "keep custom field");
    }
}

#[test]
fn preserves_utf16_xmp_and_replaces_duplicate_attribute_and_element_values() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.pdf");
    let output = dir.path().join("edited.pdf");
    let xml = XMP.replace(
        "custom:Attribute=\"keep attribute\"",
        "custom:Attribute=\"keep attribute\" d:title=\"duplicate title\"",
    );
    make_pdf(&input, Some(&xml));
    let mut doc = Document::load(&input).unwrap();
    let id = doc
        .catalog()
        .unwrap()
        .get(b"Metadata")
        .unwrap()
        .as_reference()
        .unwrap();
    let mut bytes = b"\xFF\xFE".to_vec();
    for unit in xml.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    doc.get_object_mut(id)
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .set_plain_content(bytes);
    doc.save(&input).unwrap();
    assert_success(edit(
        &input,
        Some(&output),
        &["--doc-title", "UTF-16 新標題"],
    ));
    let props = read_pdf_properties(&output, None).unwrap();
    assert_eq!(props.document_info.title.as_deref(), Some("UTF-16 新標題"));
    assert_eq!(
        props.xmp_properties["{http://purl.org/dc/elements/1.1/}title"],
        vec!["UTF-16 新標題"]
    );
    assert_eq!(props.xmp_status, pdf_encrypt::MetadataStatus::Partial); // Unrelated structured metadata is retained.
    assert!(metadata(&Document::load(&output).unwrap()).starts_with(b"\xFF\xFE"));
}

#[test]
fn edits_encrypted_files_without_changing_passwords_permissions_or_page_content() {
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("plain.pdf");
    make_pdf(&plain, Some(XMP));
    for algorithm in [
        EncryptionAlgorithm::Aes256,
        EncryptionAlgorithm::Aes128,
        EncryptionAlgorithm::Rc4Bit128,
        EncryptionAlgorithm::Rc4Bit40,
    ] {
        for open_password in ["viewer", ""] {
            let input = dir.path().join("encrypted.pdf");
            let output = dir.path().join("edited.pdf");
            let mut options = EncryptOptions::with_read_only_password(open_password, "owner");
            options.algorithm = algorithm;
            encrypt_pdf_with_options(&plain, &input, &options).unwrap();
            let before = fs::read(&input).unwrap();
            let before_doc = Document::load_with_password(&input, open_password).unwrap();
            let before_state = before_doc.encryption_state.as_ref().unwrap().clone();
            assert_success(edit(
                &input,
                Some(&output),
                &[
                    "--force",
                    "--password",
                    open_password,
                    "--doc-title",
                    "加密新標題",
                ],
            ));
            assert_eq!(fs::read(&input).unwrap(), before);
            let after = Document::load_with_password(&output, open_password).unwrap();
            let state = after.encryption_state.as_ref().unwrap();
            assert_eq!(state.version(), before_state.version());
            assert_eq!(state.revision(), before_state.revision());
            assert_eq!(state.permissions(), before_state.permissions());
            assert_eq!(state.owner_value(), before_state.owner_value());
            assert_eq!(state.user_value(), before_state.user_value());
            assert_eq!(
                state.file_encryption_key(),
                before_state.file_encryption_key()
            );
            assert_eq!(
                before_doc
                    .get_page_content(*before_doc.get_pages().values().next().unwrap())
                    .unwrap(),
                after
                    .get_page_content(*after.get_pages().values().next().unwrap())
                    .unwrap()
            );
            assert_eq!(
                read_pdf_properties(&output, Some(open_password))
                    .unwrap()
                    .document_info
                    .title
                    .as_deref(),
                Some("加密新標題")
            );
            if !open_password.is_empty() {
                assert!(Document::load_with_password(&output, "wrong").is_err());
            }
        }
    }
}

#[test]
fn preserves_aes256_encryption_and_unencrypted_metadata_with_an_owner_password() {
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("plain.pdf");
    let input = dir.path().join("encrypted.pdf");
    let output = dir.path().join("edited.pdf");
    make_pdf(&plain, None);
    let mut options = EncryptOptions::with_read_only_password("viewer", "owner");
    options.encrypt_metadata = false;
    options.document_info.title = Some("Old title".into());
    encrypt_pdf_with_options(&plain, &input, &options).unwrap();
    assert_success(edit(
        &input,
        Some(&output),
        &["-p", "owner", "--doc-title", "新標題"],
    ));
    let doc = Document::load_with_password(&output, "viewer").unwrap();
    assert!(doc.was_encrypted());
    assert!(!doc.encryption_state.as_ref().unwrap().encrypt_metadata());
    assert_eq!(
        read_pdf_properties(&output, Some("owner"))
            .unwrap()
            .document_info
            .title
            .as_deref(),
        Some("新標題")
    );
}

#[test]
fn requires_explicit_overwrite_and_supports_atomic_in_place_editing() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.pdf");
    make_pdf(&input, Some(XMP));
    let original = fs::read(&input).unwrap();
    assert!(!edit(&input, None, &["--in-place", "--doc-title", "new"])
        .status
        .success());
    assert_eq!(fs::read(&input).unwrap(), original);
    assert!(
        !edit(&input, Some(&input), &["--force", "--doc-title", "new"])
            .status
            .success()
    );
    assert_eq!(fs::read(&input).unwrap(), original);
    assert_success(edit(&input, None, &["--doc-title", "default output"]));
    let output = dir.path().join("input_edited.pdf");
    let existing = fs::read(&output).unwrap();
    assert!(!edit(&input, None, &["--doc-title", "cancel overwrite"])
        .status
        .success());
    assert_eq!(fs::read(&output).unwrap(), existing);
    assert_success(edit(
        &input,
        None,
        &["--in-place", "--force", "--doc-title", "原地新標題"],
    ));
    assert_eq!(
        read_pdf_properties(&input, None)
            .unwrap()
            .document_info
            .title
            .as_deref(),
        Some("原地新標題")
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[test]
fn failures_do_not_create_output_or_overwrite_existing_files() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("bad-xmp.pdf");
    let output = dir.path().join("edited.pdf");
    make_pdf(&input, Some("<broken>"));
    let original = fs::read(&input).unwrap();
    assert!(!edit(&input, Some(&output), &["--doc-title", "new"])
        .status
        .success());
    assert!(!output.exists());
    fs::write(&output, b"existing output").unwrap();
    assert!(
        !edit(&input, Some(&output), &["--force", "--doc-title", "new"])
            .status
            .success()
    );
    assert_eq!(fs::read(&output).unwrap(), b"existing output");
    assert!(!edit(
        &input,
        None,
        &["--in-place", "--force", "--doc-title", "new"]
    )
    .status
    .success());
    assert_eq!(fs::read(&input).unwrap(), original);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[test]
fn rejects_missing_fields_conflicting_output_flags_and_wrong_passwords() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.pdf");
    let output = dir.path().join("edited.pdf");
    make_pdf(&input, None);
    assert!(!edit(&input, Some(&output), &[]).status.success());
    assert!(
        !edit(&input, Some(&output), &["--in-place", "--doc-title", "new"])
            .status
            .success()
    );
    let encrypted = dir.path().join("encrypted.pdf");
    encrypt_pdf_with_options(
        &input,
        &encrypted,
        &EncryptOptions::with_open_password("secret"),
    )
    .unwrap();
    assert!(!edit(
        &encrypted,
        Some(&output),
        &["--password", "wrong", "--doc-title", "new"]
    )
    .status
    .success());
    assert!(!output.exists());
}

#[test]
fn changing_description_preserves_the_subject_in_info_and_xmp_only_files() {
    let dir = tempfile::tempdir().unwrap();
    for has_info in [true, false] {
        let input = dir.path().join("input.pdf");
        let output = dir.path().join("edited.pdf");
        make_pdf(&input, Some(XMP));
        if !has_info {
            let mut doc = Document::load(&input).unwrap();
            doc.trailer.remove(b"Info");
            doc.save(&input).unwrap();
        }
        let before = read_pdf_properties(&input, None)
            .unwrap()
            .document_info
            .subject;
        assert_success(edit(
            &input,
            Some(&output),
            &["--force", "--doc-description", "新描述"],
        ));
        let after = read_pdf_properties(&output, None).unwrap();
        assert_eq!(after.document_info.subject, before);
        assert_eq!(after.document_info.description.as_deref(), Some("新描述"));
        assert_eq!(
            after.xmp_properties["{https://github.com/doggy8088/pdf-encrypt/ns/1.0/}Subject"],
            vec![before.unwrap()]
        );
    }
}

#[test]
fn rejects_malformed_info_empty_xmp_and_invalid_xml_text_without_writing() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.pdf");
    let output = dir.path().join("edited.pdf");
    make_pdf(&input, Some(""));
    assert!(!edit(&input, Some(&output), &["--doc-title", "new"])
        .status
        .success());
    assert!(!output.exists());
    make_pdf(&input, None);
    assert!(
        !edit(&input, Some(&output), &["--doc-title", "bad\u{1}xml"])
            .status
            .success()
    );
    assert!(!output.exists());
    let mut doc = Document::load(&input).unwrap();
    doc.trailer.set("Info", Object::Integer(42));
    doc.save(&input).unwrap();
    let before = fs::read(&input).unwrap();
    assert!(!edit(
        &input,
        None,
        &["--in-place", "--force", "--doc-title", "new"]
    )
    .status
    .success());
    assert_eq!(fs::read(&input).unwrap(), before);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn preserves_custom_aes256_crypt_filter_names() {
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("plain.pdf");
    let input = dir.path().join("encrypted.pdf");
    let output = dir.path().join("edited.pdf");
    make_pdf(&plain, None);
    encrypt_pdf_with_options(
        &plain,
        &input,
        &EncryptOptions::with_open_password("test-password"),
    )
    .unwrap();
    let mut bytes = fs::read(&input).unwrap();
    let positions: Vec<_> = bytes
        .windows(6)
        .enumerate()
        .filter_map(|(i, word)| (word == b"/StdCF").then_some(i))
        .collect();
    assert!(!positions.is_empty());
    for i in positions {
        bytes[i..i + 6].copy_from_slice(b"/OwnCF");
    }
    fs::write(&input, bytes).unwrap();
    assert_success(edit(
        &input,
        Some(&output),
        &["-p", "test-password", "--doc-title", "new"],
    ));
    let doc = Document::load_with_password(&output, "test-password").unwrap();
    let state = doc.encryption_state.as_ref().unwrap();
    assert!(state
        .encode()
        .unwrap()
        .get(b"CF")
        .unwrap()
        .as_dict()
        .unwrap()
        .has(b"OwnCF"));
    assert_eq!(
        read_pdf_properties(&output, Some("test-password"))
            .unwrap()
            .document_info
            .title
            .as_deref(),
        Some("new")
    );
}

#[cfg(unix)]
#[test]
fn handles_symlink_aliases_and_preserves_in_place_permissions() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.pdf");
    let alias = dir.path().join("alias.pdf");
    make_pdf(&input, None);
    fs::set_permissions(&input, fs::Permissions::from_mode(0o640)).unwrap();
    symlink(&input, &alias).unwrap();
    assert!(
        !edit(&input, Some(&alias), &["--force", "--doc-title", "new"])
            .status
            .success()
    );
    assert_success(edit(
        &alias,
        None,
        &["--in-place", "--force", "--doc-title", "new"],
    ));
    assert!(fs::symlink_metadata(&alias).unwrap().is_symlink());
    assert_eq!(
        fs::metadata(&input).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert_eq!(
        read_pdf_properties(&input, None)
            .unwrap()
            .document_info
            .title
            .as_deref(),
        Some("new")
    );
}
