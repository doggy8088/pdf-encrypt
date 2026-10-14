use std::fs;
use std::path::Path;
use std::process::Command;

use lopdf::{dictionary, text_string, Document, Object, Stream, StringFormat};
use pdf_encrypt::{encrypt_pdf_with_options, DocumentInfo, EncryptOptions};
use serde_json::Value;

fn make_pdf(path: &Path, info: Option<lopdf::Dictionary>, xmp: Option<&str>, compressed: bool) {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id,
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
        if compressed {
            stream.compress().unwrap();
        }
        catalog.set("Metadata", doc.add_object(stream));
    }
    let catalog_id = doc.add_object(catalog);
    doc.trailer.set("Root", catalog_id);
    if let Some(info) = info {
        let info_id = doc.add_object(info);
        doc.trailer.set("Info", info_id);
    }
    doc.save(path).unwrap();
}

fn inspect(path: &Path, password: Option<&str>) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pdf-encrypt"));
    command.args(["info", "--json", "-i"]).arg(path);
    if let Some(password) = password {
        command.args(["-p", password]);
    }
    command.output().unwrap()
}

fn properties(path: &Path, password: Option<&str>) -> Value {
    let output = inspect(path, password);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout must contain only valid JSON")
}

#[test]
fn reads_unicode_info_custom_fields_and_preserves_the_input() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("文件.pdf");
    make_pdf(
        &path,
        Some(dictionary! {
            "Title" => text_string("文件標題"),
            "Author" => text_string("作者"),
            "Subject" => text_string("主旨"),
            "Keywords" => text_string(""),
            "Description" => text_string("文件描述"),
            "CopyrightStatus" => text_string("Copyrighted"),
            "CopyrightNotice" => text_string("版權所有 © 2026"),
            "CopyrightInfoURL" => text_string("https://example.com"),
            "Creator" => text_string("Test Creator"),
            "Producer" => text_string("Test Producer"),
            "CreationDate" => text_string("D:20261014120000+08'00'"),
            "ModDate" => text_string("D:20261014130000+08'00'"),
            "Trapped" => "False", "CustomField" => text_string("custom value"),
        }),
        None,
        false,
    );
    let original = fs::read(&path).unwrap();

    let result = properties(&path, None);
    assert_eq!(result["pdf_version"], "1.7");
    assert_eq!(result["pages"], 1);
    assert_eq!(result["input_encrypted"], false);
    assert_eq!(
        result["document_info"],
        serde_json::json!({
            "title": "文件標題", "author": "作者", "subject": "主旨", "keywords": "",
            "description": "文件描述", "copyright_status": "Copyrighted",
            "copyright_notice": "版權所有 © 2026", "copyright_info_url": "https://example.com",
        })
    );
    assert_eq!(result["info_dictionary"]["Creator"], "Test Creator");
    assert_eq!(result["info_dictionary"]["Producer"], "Test Producer");
    assert_eq!(
        result["info_dictionary"]["CreationDate"],
        "D:20261014120000+08'00'"
    );
    assert_eq!(
        result["info_dictionary"]["ModDate"],
        "D:20261014130000+08'00'"
    );
    assert_eq!(result["info_dictionary"]["Trapped"], "False");
    assert_eq!(result["info_dictionary"]["CustomField"], "custom value");
    assert_eq!(result["info_status"], "read");
    assert_eq!(result["xmp_status"], "absent");
    assert_eq!(result["warnings"], serde_json::json!([]));
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

const XMP: &str = r#"<m:xmpmeta xmlns:m="adobe:ns:meta/">
<r:RDF xmlns:r="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<r:Description r:about="" xmlns:d="http://purl.org/dc/elements/1.1/"
 xmlns:p="http://ns.adobe.com/pdf/1.3/" xmlns:rights="http://ns.adobe.com/xap/1.0/rights/"
 xmlns:x="http://ns.adobe.com/xap/1.0/" p:Producer="XMP Producer" rights:Marked="True">
 <d:title><r:Alt><r:li xml:lang="zh-TW">中文標題</r:li><r:li xml:lang="x-default">XMP &amp; Title</r:li></r:Alt></d:title>
 <d:creator><r:Seq><r:li>作者甲</r:li><r:li>作者乙</r:li></r:Seq></d:creator>
 <d:description><r:Alt><r:li xml:lang="x-default">XMP 主旨與描述</r:li></r:Alt></d:description>
 <d:subject><r:Bag><r:li>PDF</r:li><r:li>測試</r:li></r:Bag></d:subject>
 <d:rights><r:Alt><r:li>版權所有</r:li></r:Alt></d:rights>
 <rights:WebStatement r:resource="https://example.com/copyright"/>
 <x:CreateDate>2026-10-14T12:00:00+08:00</x:CreateDate>
</r:Description></r:RDF></m:xmpmeta>"#;

#[test]
fn reads_compressed_xmp_using_namespaces_attributes_arrays_and_resources() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("xmp.pdf");
    make_pdf(&path, None, Some(XMP), true);
    let result = properties(&path, None);
    assert_eq!(
        result["document_info"],
        serde_json::json!({
            "title": "XMP & Title", "author": "作者甲, 作者乙", "subject": "XMP 主旨與描述",
            "keywords": "PDF, 測試", "description": "XMP 主旨與描述", "copyright_status": "Copyrighted",
            "copyright_notice": "版權所有", "copyright_info_url": "https://example.com/copyright",
        })
    );
    assert_eq!(result["info_status"], "absent");
    assert_eq!(result["xmp_status"], "read");
    assert_eq!(
        result["xmp_properties"]["{http://ns.adobe.com/pdf/1.3/}Producer"],
        serde_json::json!(["XMP Producer"])
    );
    assert_eq!(
        result["xmp_properties"]["{http://ns.adobe.com/xap/1.0/}CreateDate"],
        serde_json::json!(["2026-10-14T12:00:00+08:00"])
    );
}

#[test]
fn distinguishes_missing_empty_and_conflicting_properties() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("conflict.pdf");
    make_pdf(
        &path,
        Some(dictionary! {
            "Title" => text_string("Info Title"), "Author" => text_string(""),
        }),
        Some(XMP),
        false,
    );
    let result = properties(&path, None);
    assert_eq!(result["document_info"]["title"], "Info Title");
    assert_eq!(result["document_info"]["author"], "");
    assert_eq!(result["document_info"]["keywords"], "PDF, 測試");
    let conflicts = result["conflicts"].as_array().unwrap();
    assert!(conflicts.iter().any(|value| value
        == &serde_json::json!({
            "field": "title", "info": "Info Title", "xmp": "XMP & Title",
        })));
    assert!(conflicts.iter().any(|value| value["field"] == "author"));

    let empty_path = dir.path().join("empty.pdf");
    make_pdf(&empty_path, None, None, false);
    let empty = properties(&empty_path, None);
    assert!(empty["document_info"]
        .as_object()
        .unwrap()
        .values()
        .all(Value::is_null));
    assert_eq!(empty["info_status"], "absent");
    assert_eq!(empty["xmp_status"], "absent");
}

#[test]
fn reports_bad_metadata_without_discarding_readable_info() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad-xmp.pdf");
    make_pdf(
        &path,
        Some(dictionary! {
            "Title" => text_string("Readable title"), "Author" => 42,
        }),
        Some("<broken>"),
        false,
    );
    let result = properties(&path, None);
    assert_eq!(result["document_info"]["title"], "Readable title");
    assert!(result["document_info"]["author"].is_null());
    assert_eq!(result["info_status"], "partial");
    assert_eq!(result["xmp_status"], "unreadable");
    assert!(result["warnings"].as_array().unwrap().len() >= 2);
}

#[test]
fn reads_encrypted_properties_and_rejects_unreliable_legacy_owner_decryption() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain.pdf");
    make_pdf(&path, None, None, false);
    for algorithm in [
        pdf_encrypt::EncryptionAlgorithm::Aes256,
        pdf_encrypt::EncryptionAlgorithm::Aes128,
        pdf_encrypt::EncryptionAlgorithm::Rc4Bit128,
        pdf_encrypt::EncryptionAlgorithm::Rc4Bit40,
    ] {
        for open_password in ["viewer", ""] {
            let encrypted_path = dir.path().join("encrypted.pdf");
            let mut options = EncryptOptions::with_open_password(open_password);
            options.owner_password = "owner".into();
            options.algorithm = algorithm;
            options.document_info = DocumentInfo {
                title: Some("加密文件標題".into()),
                ..DocumentInfo::default()
            };
            encrypt_pdf_with_options(&path, &encrypted_path, &options).unwrap();
            let before = fs::read(&encrypted_path).unwrap();
            for password in [Some(open_password), Some("owner")] {
                if algorithm != pdf_encrypt::EncryptionAlgorithm::Aes256
                    && !open_password.is_empty()
                    && password == Some("owner")
                {
                    let failed = inspect(&encrypted_path, password);
                    assert!(!failed.status.success());
                    assert!(failed.stdout.is_empty());
                    assert!(String::from_utf8_lossy(&failed.stderr).contains("open password"));
                    continue;
                }
                let result = properties(&encrypted_path, password);
                assert_eq!(
                    result["document_info"]["title"],
                    "加密文件標題",
                    "algorithm={algorithm:?}, open password empty={}, reading with owner={}",
                    open_password.is_empty(),
                    password == Some("owner")
                );
                assert_eq!(result["input_encrypted"], true);
                assert_eq!(result["xmp_status"], "read");
            }
            if open_password.is_empty() {
                assert_eq!(
                    properties(&encrypted_path, None)["document_info"]["title"],
                    "加密文件標題"
                );
            } else {
                for password in [None, Some("wrong")] {
                    let failed = inspect(&encrypted_path, password);
                    assert!(!failed.status.success());
                    assert!(failed.stdout.is_empty());
                    assert!(String::from_utf8_lossy(&failed.stderr).contains("password"));
                }
            }
            assert_eq!(fs::read(&encrypted_path).unwrap(), before);
        }
    }
}

#[test]
fn handles_indirect_info_values_and_pdf_text_encodings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("encodings.pdf");
    make_pdf(&path, None, None, false);
    let mut doc = Document::load(&path).unwrap();
    let title = doc.add_object(text_string("間接標題"));
    let mut utf8 = b"\xEF\xBB\xBF".to_vec();
    utf8.extend_from_slice("UTF-8 作者".as_bytes());
    let info = doc.add_object(dictionary! {
        "Title" => title,
        "Author" => Object::String(utf8, StringFormat::Hexadecimal),
        "Subject" => Object::String(b"PDF\x8B".to_vec(), StringFormat::Literal),
    });
    doc.trailer.set("Info", info);
    doc.save(&path).unwrap();
    let result = properties(&path, None);
    assert_eq!(result["document_info"]["title"], "間接標題");
    assert_eq!(result["document_info"]["author"], "UTF-8 作者");
    assert_eq!(result["document_info"]["subject"], "PDF‰");
}

#[test]
fn supports_text_output_alias_and_invalid_file_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain.pdf");
    make_pdf(
        &path,
        Some(dictionary! { "Title" => text_string("顯示標題") }),
        None,
        false,
    );
    let output = Command::new(env!("CARGO_BIN_EXE_pdf-encrypt"))
        .args(["properties", "-i"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("顯示標題"));
    assert!(text.contains("title:"));
    assert!(text.contains("pages: 1"));
    for path in [
        dir.path().join("missing.pdf"),
        dir.path().join("invalid.pdf"),
    ] {
        if path.file_stem().unwrap() == "invalid" {
            fs::write(&path, "not a PDF").unwrap();
        }
        let result = inspect(&path, None);
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert!(!result.stderr.is_empty());
    }
}

#[test]
fn reads_utf16_xmp_and_resource_identifiers_without_mistaking_nested_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("utf16.pdf");
    let xmp = XMP.replace("r:about=\"\"", "r:about=\"uuid:document-identifier\"")
        .replace("</r:Description>", r#"<x:Complex><r:Description><d:title>Nested title</d:title></r:Description></x:Complex>
<x:Qualified r:parseType="Resource"><r:value>Qualified value</r:value><x:Qualifier>extra</x:Qualifier></x:Qualified>
</r:Description>"#);
    make_pdf(&path, None, Some(&xmp), false);
    let mut doc = Document::load(&path).unwrap();
    let metadata_id = doc
        .catalog()
        .unwrap()
        .get(b"Metadata")
        .unwrap()
        .as_reference()
        .unwrap();
    let mut bytes = b"\xFF\xFE".to_vec();
    for unit in xmp.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    doc.get_object_mut(metadata_id)
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .set_plain_content(bytes);
    doc.save(&path).unwrap();

    let result = properties(&path, None);
    assert_eq!(result["document_info"]["title"], "XMP & Title");
    assert_eq!(result["xmp_status"], "partial");
    assert_eq!(
        result["xmp_properties"]["{http://ns.adobe.com/xap/1.0/}Qualified"],
        serde_json::json!(["Qualified value"])
    );
    assert!(
        result["xmp_properties"]["{http://ns.adobe.com/xap/1.0/}Complex"][0]
            .as_str()
            .unwrap()
            .contains("Nested title")
    );
    assert!(!result["warnings"].as_array().unwrap().is_empty());
}

#[test]
fn distinguishes_absent_metadata_from_unreadable_metadata_references() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("references.pdf");
    make_pdf(&path, None, None, false);
    let mut doc = Document::load(&path).unwrap();
    doc.trailer.set("Info", Object::Null);
    doc.catalog_mut().unwrap().set("Metadata", Object::Null);
    doc.save(&path).unwrap();
    let absent = properties(&path, None);
    assert_eq!(absent["info_status"], "absent");
    assert_eq!(absent["xmp_status"], "absent");
    assert_eq!(absent["warnings"], serde_json::json!([]));

    doc.trailer.set("Info", Object::Reference((999, 0)));
    doc.catalog_mut()
        .unwrap()
        .set("Metadata", Object::Reference((998, 0)));
    doc.save(&path).unwrap();
    let unreadable = properties(&path, None);
    assert_eq!(unreadable["info_status"], "unreadable");
    assert_eq!(unreadable["xmp_status"], "unreadable");
    assert_eq!(unreadable["warnings"].as_array().unwrap().len(), 2);
}

#[test]
fn library_api_reads_direct_info_and_preserves_invalid_text_as_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("direct-info.pdf");
    make_pdf(&path, None, None, false);
    let mut doc = Document::load(&path).unwrap();
    doc.trailer.set(
        "Info",
        dictionary! {
            "Title" => text_string("直接文件資訊"),
            "Author" => Object::String(vec![0xFE, 0xFF, 0xD8, 0x00], StringFormat::Hexadecimal),
            "Subject" => Object::Null,
        },
    );
    doc.save(&path).unwrap();
    let result = pdf_encrypt::read_pdf_properties(&path, None).unwrap();
    assert_eq!(result.document_info.title.as_deref(), Some("直接文件資訊"));
    assert_eq!(result.document_info.author, None);
    assert_eq!(result.document_info.subject, None);
    assert_eq!(result.info_status, pdf_encrypt::MetadataStatus::Partial);
    assert_eq!(result.warnings.len(), 1);
}
