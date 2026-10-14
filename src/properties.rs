use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{bail, Context, Result};
use lopdf::encryption::PasswordAlgorithm;
use lopdf::{decode_text_string, Document, Object};
use roxmltree::Node;
use serde::Serialize;

use crate::DocumentInfo;

pub(crate) const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
pub(crate) const DC: &str = "http://purl.org/dc/elements/1.1/";
pub(crate) const PDF: &str = "http://ns.adobe.com/pdf/1.3/";
pub(crate) const RIGHTS: &str = "http://ns.adobe.com/xap/1.0/rights/";
pub(crate) const PDF_ENCRYPT: &str = "https://github.com/doggy8088/pdf-encrypt/ns/1.0/";

/// Whether a metadata source was absent, read, partially read, or unreadable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataStatus {
    Absent,
    Read,
    Partial,
    Unreadable,
}

/// Different values for the same document field in Info and XMP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PropertyConflict {
    pub field: String,
    pub info: String,
    pub xmp: String,
}

/// Read-only document properties, their sources, and any extraction problems.
#[derive(Debug, Clone, Serialize)]
pub struct PdfProperties {
    pub pdf_version: String,
    pub input_encrypted: bool,
    pub pages: usize,
    /// Suggested defaults: Info values take precedence over XMP, including empty strings.
    pub document_info: DocumentInfo,
    /// All readable scalar entries, including Creator, Producer, dates, and custom fields.
    pub info_dictionary: BTreeMap<String, String>,
    /// XMP keys use `{namespace URI}local-name`; arrays retain their individual values.
    /// Structured values that cannot be reduced to text retain their XML representation.
    pub xmp_properties: BTreeMap<String, Vec<String>>,
    pub info_status: MetadataStatus,
    pub xmp_status: MetadataStatus,
    pub conflicts: Vec<PropertyConflict>,
    pub warnings: Vec<String>,
}

/// Read PDF properties without changing the file or creating a decrypted copy.
///
/// `None` tries an empty password, supporting plain and owner-password-only PDFs.
/// Password-protected input requires a valid password. AES-256 supports user and owner
/// passwords; older profiles require the user password unless it is empty, because
/// lopdf 0.40 derives an incorrect decryption key from legacy owner passwords. Invalid input
/// and authentication failures return errors; damaged metadata is reported through
/// source statuses and warnings while other readable properties are retained.
pub fn read_pdf_properties<P: AsRef<Path>>(
    input_path: P,
    password: Option<&str>,
) -> Result<PdfProperties> {
    let input_path = input_path.as_ref();
    let doc = Document::load_with_password(input_path, password.unwrap_or_default())
        .with_context(|| format!("Failed to open PDF file: {}", input_path.display()))?;
    if doc.is_encrypted() {
        bail!(
            "PDF requires a valid user or owner password; provide --password (-p): {}",
            input_path.display()
        );
    }
    validate_decryption_key(&doc, password.unwrap_or_default())?;

    Ok(properties_from_document(&doc))
}

pub(crate) fn properties_from_document(doc: &Document) -> PdfProperties {
    let mut warnings = Vec::new();
    let (info_dictionary, info_status) = read_info_dictionary(doc, &mut warnings);
    let (xmp, xmp_status) = read_xmp(doc, &mut warnings);
    let info = info_defaults(&info_dictionary);
    let mut conflicts = Vec::new();
    let document_info = merge_defaults(info, xmp_defaults(&xmp.text_values), &mut conflicts);

    PdfProperties {
        pdf_version: doc.version.clone(),
        input_encrypted: doc.was_encrypted(),
        pages: doc.get_pages().len(),
        document_info,
        info_dictionary,
        xmp_properties: xmp.properties,
        info_status,
        xmp_status,
        conflicts,
        warnings,
    }
}

pub(crate) fn validate_decryption_key(doc: &Document, password: &str) -> Result<()> {
    let Some(state) = &doc.encryption_state else {
        return Ok(());
    };
    // Loading removes /Encrypt. Reconstruct only the authentication context so we
    // can verify the key before returning values that lopdf may have mis-decrypted.
    let mut context = Document::new();
    context.trailer = doc.trailer.clone();
    let encrypt_id = context.add_object(state.encode()?);
    context.trailer.set("Encrypt", encrypt_id);
    let algorithm = PasswordAlgorithm::try_from(&context)?;
    let password = if context.authenticate_user_password("").is_ok() {
        ""
    } else {
        password
    };
    let password = algorithm.sanitize_password(password)?;
    if state.revision() <= 4
        && algorithm
            .authenticate_user_password(&context, &password)
            .is_err()
    {
        bail!("This PDF uses AES-128 or RC4: the current PDF backend cannot reliably read it with an owner password. Provide the document open password with --password (-p)");
    }
    let expected = algorithm.compute_file_encryption_key(&context, &password)?;
    if expected.as_slice() != state.file_encryption_key() {
        bail!("The current PDF backend cannot reliably decode this password's encoding; document properties were not read");
    }
    Ok(())
}

fn read_info_dictionary(
    doc: &Document,
    warnings: &mut Vec<String>,
) -> (BTreeMap<String, String>, MetadataStatus) {
    let mut fields = BTreeMap::new();
    let Ok(object) = doc.trailer.get(b"Info") else {
        return (fields, MetadataStatus::Absent);
    };
    let dictionary = match doc.dereference(object) {
        Ok((_, Object::Null)) => return (fields, MetadataStatus::Absent),
        Ok((_, Object::Dictionary(dictionary))) => dictionary,
        Ok(_) => {
            warnings.push("Cannot read Document Info: /Info is not a dictionary".to_owned());
            return (fields, MetadataStatus::Unreadable);
        }
        Err(error) => {
            warnings.push(format!("Cannot read Document Info dictionary: {error}"));
            return (fields, MetadataStatus::Unreadable);
        }
    };

    let mut status = MetadataStatus::Read;
    for (key, value) in dictionary.iter() {
        let key = String::from_utf8_lossy(key).into_owned();
        let value = doc.dereference(value).map(|(_, object)| object);
        let text = value.map_err(anyhow::Error::from).and_then(|object| {
            if matches!(object, Object::Null) {
                return Ok(None);
            }
            if is_document_field(&key) && !matches!(object, Object::String(..)) {
                bail!("document field is not a PDF text string");
            }
            let value = match object {
                Object::String(..) => decode_pdf_text(object)?,
                Object::Name(value) => String::from_utf8(value.clone())?,
                Object::Boolean(value) => value.to_string(),
                Object::Integer(value) => value.to_string(),
                Object::Real(value) => value.to_string(),
                _ => bail!("unsupported non-scalar Info value"),
            };
            Ok(Some(value))
        });
        match text {
            Ok(Some(value)) => {
                fields.insert(key, value);
            }
            Ok(None) => {}
            Err(error) => {
                status = MetadataStatus::Partial;
                warnings.push(format!("Cannot read Document Info field {key}: {error}"));
            }
        }
    }
    (fields, status)
}

fn is_document_field(key: &str) -> bool {
    matches!(
        key,
        "Title"
            | "Author"
            | "Subject"
            | "Keywords"
            | "Description"
            | "CopyrightStatus"
            | "CopyrightNotice"
            | "CopyrightInfoURL"
    )
}

fn decode_pdf_text(object: &Object) -> Result<String> {
    let bytes = object.as_str()?;
    // lopdf 0.40's UTF-8 decoder retains the encoding BOM as part of the value.
    if let Some(text) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        return Ok(std::str::from_utf8(text)?.to_owned());
    }
    if let Some(text) = bytes.strip_prefix(b"\xFF\xFE") {
        return decode_utf16(text, true);
    }
    if let Some(text) = bytes.strip_prefix(b"\xFE\xFF") {
        return decode_utf16(text, false);
    }
    Ok(decode_text_string(object)?)
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> Result<String> {
    let (pairs, remainder) = bytes.as_chunks::<2>();
    if !remainder.is_empty() {
        bail!("invalid UTF-16 text length");
    }
    let units: Vec<_> = pairs
        .iter()
        .map(|pair| {
            if little_endian {
                u16::from_le_bytes([pair[0], pair[1]])
            } else {
                u16::from_be_bytes([pair[0], pair[1]])
            }
        })
        .collect();
    Ok(String::from_utf16(&units)?)
}

#[derive(Default)]
struct ParsedXmp {
    properties: BTreeMap<String, Vec<String>>,
    text_values: BTreeMap<String, String>,
}

fn read_xmp(doc: &Document, warnings: &mut Vec<String>) -> (ParsedXmp, MetadataStatus) {
    let catalog = match doc.catalog() {
        Ok(catalog) => catalog,
        Err(error) => {
            warnings.push(format!("Cannot read PDF catalog for XMP: {error}"));
            return (ParsedXmp::default(), MetadataStatus::Unreadable);
        }
    };
    let Ok(object) = catalog.get(b"Metadata") else {
        return (ParsedXmp::default(), MetadataStatus::Absent);
    };
    if matches!(doc.dereference(object), Ok((_, Object::Null))) {
        return (ParsedXmp::default(), MetadataStatus::Absent);
    }
    let warning_count = warnings.len();
    let result = (|| -> Result<ParsedXmp> {
        let stream = doc.dereference(object)?.1.as_stream()?;
        let content = match stream.dict.get(b"Filter") {
            Err(_) | Ok(Object::Null) => stream.content.clone(),
            Ok(_) => stream
                .decompressed_content()
                .context("Cannot decompress XMP stream")?,
        };
        let text = decode_xml(&content)?;
        parse_xmp(&text, warnings)
    })();
    match result {
        Ok(xmp) => (
            xmp,
            if warnings.len() == warning_count {
                MetadataStatus::Read
            } else {
                MetadataStatus::Partial
            },
        ),
        Err(error) => {
            warnings.push(format!("Cannot read XMP metadata: {error:#}"));
            (ParsedXmp::default(), MetadataStatus::Unreadable)
        }
    }
}

pub(crate) fn decode_xml(bytes: &[u8]) -> Result<String> {
    if let Some(text) = bytes.strip_prefix(b"\xFF\xFE") {
        decode_utf16(text, true)
    } else if let Some(text) = bytes.strip_prefix(b"\xFE\xFF") {
        decode_utf16(text, false)
    } else if bytes.starts_with(b"\x00<\x00?") {
        decode_utf16(bytes, false)
    } else if bytes.starts_with(b"<\x00?\x00") {
        decode_utf16(bytes, true)
    } else {
        Ok(std::str::from_utf8(bytes)?
            .trim_start_matches('\u{feff}')
            .to_owned())
    }
}

fn parse_xmp(text: &str, warnings: &mut Vec<String>) -> Result<ParsedXmp> {
    let xml = roxmltree::Document::parse(text).context("Invalid XMP XML")?;
    let rdf = xml
        .descendants()
        .find(|node| node.has_tag_name((RDF, "RDF")))
        .context("XMP does not contain rdf:RDF")?;
    let mut xmp = ParsedXmp::default();
    for description in rdf
        .children()
        .filter(|node| node.has_tag_name((RDF, "Description")))
    {
        for attribute in description.attributes() {
            if let Some(namespace) = attribute.namespace() {
                if namespace != RDF && namespace != roxmltree::NS_XML_URI {
                    let value = attribute.value().to_owned();
                    insert_xmp(
                        &mut xmp,
                        namespace,
                        attribute.name(),
                        vec![value.clone()],
                        Some(value),
                        warnings,
                    );
                }
            }
        }
        for property in description.children().filter(Node::is_element) {
            let tag = property.tag_name();
            let namespace = tag.namespace().unwrap_or_default();
            let container = property.children().find(|node| {
                node.has_tag_name((RDF, "Alt"))
                    || node.has_tag_name((RDF, "Seq"))
                    || node.has_tag_name((RDF, "Bag"))
            });
            let (values, default) = if let Some(container) = container {
                let mut items: Vec<_> = container
                    .children()
                    .filter(|node| node.has_tag_name((RDF, "li")))
                    .collect();
                let is_alt = container.has_tag_name((RDF, "Alt"));
                if is_alt {
                    items.sort_by_key(|node| {
                        node.attribute((roxmltree::NS_XML_URI, "lang")) != Some("x-default")
                    });
                }
                let texts: Option<Vec<_>> = items.iter().map(|node| literal_value(*node)).collect();
                let values: Vec<_> = items
                    .iter()
                    .map(|node| {
                        literal_value(*node).unwrap_or_else(|| text[node.range()].to_owned())
                    })
                    .collect();
                let default = texts.map(|texts| {
                    if is_alt {
                        texts.first().cloned().unwrap_or_default()
                    } else {
                        texts.join(", ")
                    }
                });
                (values, default)
            } else {
                let value = literal_value(property);
                (
                    vec![value
                        .clone()
                        .unwrap_or_else(|| text[property.range()].to_owned())],
                    value,
                )
            };
            insert_xmp(&mut xmp, namespace, tag.name(), values, default, warnings);
        }
    }
    Ok(xmp)
}

fn literal_value(node: Node<'_, '_>) -> Option<String> {
    if let Some(value) = node.attribute((RDF, "resource")) {
        return Some(value.to_owned());
    }
    if let Some(value) = node
        .children()
        .find(|node| node.has_tag_name((RDF, "value")))
    {
        return literal_value(value);
    }
    if node.children().any(|child| child.is_element()) {
        return None;
    }
    Some(
        node.children()
            .filter(Node::is_text)
            .filter_map(|node| node.text())
            .collect(),
    )
}

fn insert_xmp(
    xmp: &mut ParsedXmp,
    namespace: &str,
    name: &str,
    values: Vec<String>,
    default: Option<String>,
    warnings: &mut Vec<String>,
) {
    let key = format!("{{{namespace}}}{name}");
    xmp.properties
        .entry(key.clone())
        .or_default()
        .extend(values);
    if let Some(value) = default {
        if let Some(previous) = xmp.text_values.get(&key) {
            if previous != &value {
                warnings.push(format!("XMP property {key} has multiple different values; the first is used as the default"));
            }
        } else {
            xmp.text_values.insert(key, value);
        }
    } else {
        warnings.push(format!(
            "XMP property {key} is structured; its XML is retained without a text default"
        ));
    }
}

fn info_defaults(fields: &BTreeMap<String, String>) -> DocumentInfo {
    DocumentInfo {
        title: fields.get("Title").cloned(),
        author: fields.get("Author").cloned(),
        subject: fields.get("Subject").cloned(),
        keywords: fields.get("Keywords").cloned(),
        description: fields.get("Description").cloned(),
        copyright_status: fields.get("CopyrightStatus").cloned(),
        copyright_notice: fields.get("CopyrightNotice").cloned(),
        copyright_info_url: fields.get("CopyrightInfoURL").cloned(),
    }
}

fn xmp_defaults(fields: &BTreeMap<String, String>) -> DocumentInfo {
    let get = |namespace, name| fields.get(&format!("{{{namespace}}}{name}")).cloned();
    let copyright_status = get(PDF_ENCRYPT, "CopyrightStatus").or_else(|| {
        get(RIGHTS, "Marked").and_then(|value| match value.trim().to_ascii_lowercase().as_str() {
            "true" | "1" => Some("Copyrighted".to_owned()),
            "false" | "0" => Some("Public Domain".to_owned()),
            _ => None,
        })
    });
    DocumentInfo {
        title: get(DC, "title"),
        author: get(DC, "creator"),
        subject: get(PDF_ENCRYPT, "Subject").or_else(|| get(DC, "description")),
        keywords: get(PDF, "Keywords").or_else(|| get(DC, "subject")),
        description: get(DC, "description"),
        copyright_status,
        copyright_notice: get(DC, "rights"),
        copyright_info_url: get(RIGHTS, "WebStatement"),
    }
}

fn merge_defaults(
    info: DocumentInfo,
    xmp: DocumentInfo,
    conflicts: &mut Vec<PropertyConflict>,
) -> DocumentInfo {
    macro_rules! merge {
        ($field:ident) => {{
            if let (Some(info), Some(xmp)) = (&info.$field, &xmp.$field) {
                if info != xmp {
                    conflicts.push(PropertyConflict {
                        field: stringify!($field).to_owned(),
                        info: info.clone(),
                        xmp: xmp.clone(),
                    });
                }
            }
            info.$field.or(xmp.$field)
        }};
    }
    DocumentInfo {
        title: merge!(title),
        author: merge!(author),
        subject: merge!(subject),
        keywords: merge!(keywords),
        description: merge!(description),
        copyright_status: merge!(copyright_status),
        copyright_notice: merge!(copyright_notice),
        copyright_info_url: merge!(copyright_info_url),
    }
}
