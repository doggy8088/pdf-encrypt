//! Metadata editing that preserves unselected properties and encryption settings.

use std::fs;
use std::io::Write;
use std::ops::Range;
use std::path::Path;

use anyhow::{bail, Context, Result};
use lopdf::{dictionary, Document, Object, Stream};

use crate::properties::{
    decode_xml, properties_from_document, validate_decryption_key, DC, PDF, PDF_ENCRYPT, RDF,
    RIGHTS,
};
use crate::{
    apply_document_info, build_xmp_metadata, file_size, DocumentInfo, PdfCryptoReport, PdfOperation,
};

/// Settings for [`edit_pdf_properties`].
#[derive(Debug, Clone, Default)]
pub struct EditOptions {
    /// Existing input password. Omit for unencrypted or empty-open-password PDFs.
    pub password: Option<String>,
    /// Only provided fields are changed. An empty string explicitly clears a value.
    pub document_info: DocumentInfo,
}

/// Edit Document Info and XMP while retaining existing passwords and permissions.
///
/// The output is saved to a temporary file in its destination directory and then
/// atomically replaces the destination. Input and output may be the same path.
/// Malformed existing metadata is an error, so unrelated properties are not lost.
/// AES-128/RC4 inputs with a nonempty open password require that open password
/// rather than an owner password, matching [`crate::read_pdf_properties`].
pub fn edit_pdf_properties<I, O>(
    input_path: I,
    output_path: O,
    options: &EditOptions,
) -> Result<PdfCryptoReport>
where
    I: AsRef<Path>,
    O: AsRef<Path>,
{
    if options.document_info.is_empty() {
        bail!("At least one document property must be provided");
    }
    let input_path = input_path.as_ref();
    let output_path = output_path.as_ref();
    let input_size = file_size(input_path, "input")?;
    let password = options.password.as_deref().unwrap_or_default();
    let mut doc = Document::load_with_password(input_path, password)
        .with_context(|| format!("Failed to open PDF file: {}", input_path.display()))?;
    if doc.is_encrypted() {
        bail!("Input PDF requires a valid --password");
    }
    validate_decryption_key(&doc, password)?;
    let input_encrypted = doc.was_encrypted();
    let encryption = doc.encryption_state.clone();
    apply_document_info(&mut doc, &options.document_info)?;
    let page_count = doc.get_pages().len();
    let object_count = doc.objects.len();
    if let Some(state) = encryption {
        doc.encrypt(&state)
            .context("Failed to restore PDF encryption")?;
        // lopdf encodes Length in bits for crypt filters, but AESV3 requires bytes.
        if state.version() == 5 {
            let id = doc.trailer.get(b"Encrypt")?.as_reference()?;
            let encrypted = doc.get_dictionary_mut(id)?;
            encrypted.set("Length", 256);
            if let Ok(filters) = encrypted.get_mut(b"CF").and_then(Object::as_dict_mut) {
                for (_, filter) in filters.iter_mut() {
                    let filter = filter.as_dict_mut()?;
                    if filter.get(b"CFM").and_then(Object::as_name).ok() == Some(b"AESV3") {
                        filter.set("Length", 32);
                    }
                }
            }
        }
    }
    let output_size = save_atomically(&mut doc, input_path, output_path)?;
    Ok(PdfCryptoReport {
        operation: PdfOperation::Edit,
        input_size,
        output_size,
        input_encrypted,
        output_encrypted: input_encrypted,
        page_count,
        object_count,
        algorithm: None,
    })
}

fn save_atomically(doc: &mut Document, input: &Path, output: &Path) -> Result<u64> {
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)
        .context("Failed to create temporary PDF in output directory")?;
    doc.save_to(temp.as_file_mut())
        .context("Failed to write edited PDF")?;
    temp.as_file_mut().flush()?;
    let metadata = if output.exists() {
        fs::metadata(output)?
    } else {
        fs::metadata(input)?
    };
    temp.as_file().set_permissions(metadata.permissions())?;
    temp.as_file().sync_all()?;
    let output_size = temp.as_file().metadata()?.len();
    temp.persist(output)
        .with_context(|| format!("Failed to replace output PDF: {}", output.display()))?;
    Ok(output_size)
}

pub(crate) fn update_xmp_metadata(doc: &mut Document, info: &DocumentInfo) -> Result<()> {
    let catalog_id = doc.trailer.get(b"Root")?.as_reference()?;
    let existing = doc.catalog()?.get(b"Metadata").ok().cloned();
    let has_existing = !matches!(existing, None | Some(Object::Null));
    let mut xmp_info = info.clone();
    if info.description.is_some() && info.subject.is_none() {
        let current = properties_from_document(doc);
        // External XMP often uses dc:description for both subject and description.
        // Keep its old subject independently when only the description is edited.
        if !current
            .xmp_properties
            .contains_key(&format!("{{{PDF_ENCRYPT}}}Subject"))
        {
            xmp_info.subject = current.document_info.subject;
        }
    }
    let (id, mut stream) = match existing {
        None | Some(Object::Null) => (
            None,
            Stream::new(
                dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
                Vec::new(),
            ),
        ),
        Some(object) => {
            let (id, object) = doc
                .dereference(&object)
                .context("Cannot access existing XMP metadata")?;
            (
                id,
                object
                    .as_stream()
                    .context("Existing XMP metadata is not a stream")?
                    .clone(),
            )
        }
    };
    let original = stream
        .get_plain_content()
        .context("Cannot decode existing XMP metadata")?;
    let generated = build_xmp_metadata(&xmp_info);
    roxmltree::Document::parse(&generated)
        .context("Document properties contain invalid XML characters")?;
    let updated = if has_existing {
        let text = decode_xml(&original).context("Cannot decode existing XMP text")?;
        let merged = merge_xmp(&text, &generated, info)?;
        encode_like_original(&merged, &original)
    } else {
        generated.into_bytes()
    };
    let compressed = stream.dict.has(b"Filter");
    stream.set_plain_content(updated);
    if stream.dict.has(b"DL") {
        stream.dict.set("DL", stream.content.len() as i64);
    }
    if compressed {
        stream.compress()?;
    }
    let metadata_id = match id {
        Some(id) => {
            doc.objects.insert(id, Object::Stream(stream));
            id
        }
        None => doc.add_object(stream),
    };
    doc.get_dictionary_mut(catalog_id)?
        .set("Metadata", metadata_id);
    Ok(())
}

fn merge_xmp(text: &str, generated: &str, info: &DocumentInfo) -> Result<String> {
    let xml = roxmltree::Document::parse(text)
        .context("Invalid existing XMP XML; refusing to discard metadata")?;
    let rdf = xml
        .descendants()
        .find(|n| n.has_tag_name((RDF, "RDF")))
        .context("Existing XMP does not contain rdf:RDF")?;
    let generated_xml = roxmltree::Document::parse(generated)?;
    let description = generated_xml
        .descendants()
        .find(|n| n.has_tag_name((RDF, "Description")))
        .unwrap();
    let mut new_description = generated[description.range()].replacen(
        "<rdf:Description",
        &format!("<rdf:Description xmlns:rdf=\"{RDF}\""),
        1,
    );
    if let Some(about) = rdf
        .children()
        .find(|n| n.has_tag_name((RDF, "Description")))
        .and_then(|n| n.attribute((RDF, "about")))
    {
        new_description = new_description.replacen(
            "rdf:about=\"\"",
            &format!("rdf:about=\"{}\"", crate::escape_xml(about)),
            1,
        );
    }
    let mut changes: Vec<(Range<usize>, String)> = Vec::new();
    for description in rdf
        .children()
        .filter(|n| n.has_tag_name((RDF, "Description")))
    {
        for attribute in description.attributes() {
            if selected_property(info, attribute.namespace(), attribute.name()) {
                changes.push((attribute.range(), String::new()));
            }
        }
        for property in description.children().filter(roxmltree::Node::is_element) {
            let tag = property.tag_name();
            if selected_property(info, tag.namespace(), tag.name()) {
                changes.push((property.range(), String::new()));
            }
        }
    }
    let range = rdf.range();
    let rdf_source = &text[range.clone()];
    if rdf_source.ends_with("/>") {
        let name = rdf_source[1..]
            .split(|c: char| c.is_whitespace() || c == '/' || c == '>')
            .next()
            .unwrap();
        changes.push((
            range.end - 2..range.end,
            format!(">\n{new_description}\n</{name}>"),
        ));
    } else {
        let closing = range.start
            + rdf_source
                .rfind("</")
                .context("Missing XMP RDF closing tag")?;
        changes.push((closing..closing, format!("\n{new_description}\n")));
    }
    changes.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut merged = text.to_owned();
    for (range, replacement) in changes {
        merged.replace_range(range, &replacement);
    }
    roxmltree::Document::parse(&merged).context("Edited XMP XML is invalid")?;
    Ok(merged)
}

fn selected_property(info: &DocumentInfo, namespace: Option<&str>, name: &str) -> bool {
    match (namespace.unwrap_or_default(), name) {
        (DC, "title") => info.title.is_some(),
        (DC, "creator") => info.author.is_some(),
        (PDF_ENCRYPT, "Subject") => info.subject.is_some(),
        (PDF, "Keywords") | (DC, "subject") => info.keywords.is_some(),
        (DC, "description") => info.description.is_some(),
        (PDF_ENCRYPT, "CopyrightStatus") | (RIGHTS, "Marked") => info.copyright_status.is_some(),
        (DC, "rights") => info.copyright_notice.is_some(),
        (RIGHTS, "WebStatement") => info.copyright_info_url.is_some(),
        _ => false,
    }
}

fn encode_like_original(text: &str, original: &[u8]) -> Vec<u8> {
    let little_endian = original.starts_with(b"\xFF\xFE") || original.starts_with(b"<\x00?\x00");
    let big_endian = original.starts_with(b"\xFE\xFF") || original.starts_with(b"\x00<\x00?");
    let mut bytes = Vec::new();
    if little_endian || big_endian {
        if original.starts_with(b"\xFF\xFE") || original.starts_with(b"\xFE\xFF") {
            bytes.extend_from_slice(&original[..2]);
        }
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&if little_endian {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            });
        }
    } else {
        if original.starts_with(b"\xEF\xBB\xBF") {
            bytes.extend_from_slice(b"\xEF\xBB\xBF");
        }
        bytes.extend_from_slice(text.as_bytes());
    }
    bytes
}
