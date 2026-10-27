//! Text per page (PDF) or per slide (PPTX) with pure-Rust crates only: `pdf-extract` over
//! `lopdf` for PDF, `zip` and `quick-xml` for PPTX. No host binary, no network. Nothing here
//! looks at KPIs; `rules` does.
//!
//! Hostile input is bounded: a PDF page that panics the parser yields an empty page, a
//! presentation is read entry by entry with a size cap per entry and for the whole archive,
//! and both formats are capped in pages.

use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::panic::{catch_unwind, AssertUnwindSafe};

use quick_xml::events::Event;
use quick_xml::Reader;

/// Largest upload (the brief: 20 MB).
pub const MAX_UPLOAD_BYTES: usize = 20 * 1024 * 1024;
/// Pages or slides read at most.
pub const MAX_PAGES: usize = 500;
/// Uncompressed bytes read from one archive entry.
const MAX_ENTRY_BYTES: u64 = 8 * 1024 * 1024;
/// Uncompressed bytes read from one archive in total.
const MAX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Pdf,
    Pptx,
}

impl Format {
    pub fn ext(self) -> &'static str {
        match self {
            Format::Pdf => "pdf",
            Format::Pptx => "pptx",
        }
    }

    /// `PDF` or `PPTX`, as the document text names it.
    pub fn label(self) -> &'static str {
        match self {
            Format::Pdf => "PDF",
            Format::Pptx => "PPTX",
        }
    }

    /// `page` or `slide`: what a proposal's `page` counts.
    pub fn locator(self) -> &'static str {
        match self {
            Format::Pdf => "page",
            Format::Pptx => "slide",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum DocError {
    /// Not a readable PDF or presentation.
    Unreadable(&'static str),
    /// A password-protected PDF.
    Encrypted,
    TooManyPages,
    /// The extractor cannot run at all (the server's own binary is missing): a server
    /// fault, not a bad file.
    Unavailable(&'static str),
}

/// Every message `pages` itself gives for `DocError::Unreadable`; the child process reports
/// one of these and the parent maps it back.
pub const UNREADABLE_MESSAGES: &[&str] = &[
    "The file is not a readable PDF",
    "The file is not a readable PPTX presentation",
    "The presentation has no slides",
    "The file could not be read",
];

impl DocError {
    pub fn message(&self) -> &'static str {
        match self {
            DocError::Unreadable(m) | DocError::Unavailable(m) => m,
            DocError::Encrypted => "Password-protected PDF files are not supported",
            DocError::TooManyPages => "The document has more than 500 pages or slides",
        }
    }
}

/// The format named by the first bytes, whatever the file name says: `%PDF-` within the
/// first KiB, or a ZIP local header whose archive holds `ppt/presentation.xml`.
pub fn sniff(bytes: &[u8]) -> Option<Format> {
    let head = &bytes[..bytes.len().min(1024)];
    if head.windows(5).any(|w| w == b"%PDF-") {
        return Some(Format::Pdf);
    }
    if bytes.starts_with(b"PK\x03\x04") {
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).ok()?;
        zip.by_name("ppt/presentation.xml").ok()?;
        return Some(Format::Pptx);
    }
    None
}

/// Text of each page or slide, in order (`result[0]` is page 1). An empty string is a page
/// without extractable text (an image, a scan).
pub fn pages(format: Format, bytes: &[u8]) -> Result<Vec<String>, DocError> {
    match format {
        Format::Pdf => pdf_pages(bytes),
        Format::Pptx => pptx_pages(bytes),
    }
}

fn pdf_pages(bytes: &[u8]) -> Result<Vec<String>, DocError> {
    let unreadable = DocError::Unreadable("The file is not a readable PDF");
    let doc = match catch_unwind(AssertUnwindSafe(|| pdf_extract::Document::load_mem(bytes))) {
        Ok(Ok(doc)) => doc,
        _ => return Err(unreadable),
    };
    if doc.is_encrypted() {
        return Err(DocError::Encrypted);
    }
    let numbers: Vec<u32> = doc.get_pages().keys().copied().collect();
    if numbers.is_empty() {
        return Err(unreadable);
    }
    if numbers.len() > MAX_PAGES {
        return Err(DocError::TooManyPages);
    }
    let mut out = Vec::with_capacity(numbers.len());
    for n in numbers {
        let text = catch_unwind(AssertUnwindSafe(|| {
            let mut text = String::new();
            let mut sink = pdf_extract::PlainTextOutput::new(&mut text);
            pdf_extract::output_doc_page(&doc, &mut sink, n).map(|()| text)
        }));
        out.push(match text {
            Ok(Ok(t)) => t,
            // A page the parser cannot read is a page without text, not a failed upload.
            _ => String::new(),
        });
    }
    Ok(out)
}

fn read_entry(
    zip: &mut zip::ZipArchive<Cursor<&[u8]>>,
    name: &str,
    budget: &mut u64,
) -> Option<Vec<u8>> {
    let file = zip.by_name(name).ok()?;
    let mut buf = Vec::new();
    file.take(MAX_ENTRY_BYTES + 1).read_to_end(&mut buf).ok()?;
    if buf.len() as u64 > MAX_ENTRY_BYTES || buf.len() as u64 > *budget {
        return None;
    }
    *budget -= buf.len() as u64;
    Some(buf)
}

fn attr(e: &quick_xml::events::BytesStart<'_>, key: &[u8]) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == key)
        .and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
}

/// Slide part names in presentation order: `p:sldIdLst` resolved through the relationships;
/// the numeric order of `slideN.xml` when either file cannot be read.
fn slide_parts(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, budget: &mut u64) -> Vec<String> {
    let mut numbered: Vec<(u32, String)> = zip
        .file_names()
        .filter_map(|n| {
            let num = n.strip_prefix("ppt/slides/slide")?.strip_suffix(".xml")?;
            Some((num.parse::<u32>().ok()?, n.to_string()))
        })
        .collect();
    numbered.sort();
    let fallback: Vec<String> = numbered.into_iter().map(|(_, n)| n).collect();

    let (Some(pres), Some(rels)) = (
        read_entry(zip, "ppt/presentation.xml", budget),
        read_entry(zip, "ppt/_rels/presentation.xml.rels", budget),
    ) else {
        return fallback;
    };
    let mut targets: HashMap<String, String> = HashMap::new();
    let mut reader = Reader::from_reader(rels.as_slice());
    loop {
        match reader.read_event() {
            Ok(Event::Start(e) | Event::Empty(e)) if e.local_name().as_ref() == b"Relationship" => {
                if let (Some(id), Some(target)) = (attr(&e, b"Id"), attr(&e, b"Target")) {
                    let full = match target.strip_prefix('/') {
                        Some(abs) => abs.to_string(),
                        None => format!("ppt/{target}"),
                    };
                    targets.insert(id, full);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    let mut ordered = Vec::new();
    let mut reader = Reader::from_reader(pres.as_slice());
    loop {
        match reader.read_event() {
            Ok(Event::Start(e) | Event::Empty(e)) if e.local_name().as_ref() == b"sldId" => {
                if let Some(part) = attr(&e, b"r:id").and_then(|id| targets.get(&id)) {
                    ordered.push(part.clone());
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    if ordered.is_empty() {
        fallback
    } else {
        ordered
    }
}

/// The text of one slide: a paragraph is a line, a table row is one line with its cells
/// side by side (so a label and its figure stay together).
fn slide_text(xml: &[u8]) -> String {
    let mut out = String::new();
    let mut reader = Reader::from_reader(xml);
    let (mut in_t, mut in_cell) = (false, false);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.local_name().as_ref() {
                b"t" => in_t = true,
                b"tc" => in_cell = true,
                _ => {}
            },
            Ok(Event::Empty(e)) if e.local_name().as_ref() == b"br" => {
                out.push(if in_cell { ' ' } else { '\n' });
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"t" => in_t = false,
                b"p" => out.push(if in_cell { ' ' } else { '\n' }),
                b"tc" => {
                    in_cell = false;
                    out.push_str("  ");
                }
                b"tr" => out.push('\n'),
                _ => {}
            },
            Ok(Event::Text(t)) if in_t => {
                if let Ok(s) = t.unescape() {
                    out.push_str(&s);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

fn pptx_pages(bytes: &[u8]) -> Result<Vec<String>, DocError> {
    let unreadable = DocError::Unreadable("The file is not a readable PPTX presentation");
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| unreadable)?;
    let mut budget = MAX_ARCHIVE_BYTES;
    let parts = slide_parts(&mut zip, &mut budget);
    if parts.is_empty() {
        return Err(DocError::Unreadable("The presentation has no slides"));
    }
    if parts.len() > MAX_PAGES {
        return Err(DocError::TooManyPages);
    }
    Ok(parts
        .iter()
        .map(|p| {
            read_entry(&mut zip, p, &mut budget)
                .map(|xml| slide_text(&xml))
                .unwrap_or_default()
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slide_text_keeps_a_table_row_on_one_line() {
        let xml = br#"<p:sld xmlns:p="p" xmlns:a="a"><p:txBody>
            <a:p><a:r><a:t>Ringkasan</a:t></a:r></a:p>
            <a:p><a:r><a:t>Produksi </a:t></a:r><a:r><a:t>12,5 Mt</a:t></a:r></a:p></p:txBody>
            <a:tbl><a:tr><a:tc><a:txBody><a:p><a:r><a:t>Penjualan</a:t></a:r></a:p></a:txBody></a:tc>
            <a:tc><a:txBody><a:p><a:r><a:t>11,0 Mt</a:t></a:r></a:p></a:txBody></a:tc></a:tr></a:tbl></p:sld>"#;
        let text = slide_text(xml);
        let lines: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        assert_eq!(
            lines,
            ["Ringkasan", "Produksi 12,5 Mt", "Penjualan   11,0 Mt"]
        );
    }

    #[test]
    fn sniffing_goes_by_bytes_not_by_name() {
        assert_eq!(sniff(b"%PDF-1.4\n..."), Some(Format::Pdf));
        assert_eq!(sniff(b"junk"), None);
        assert_eq!(sniff(b""), None);
        // A ZIP that is not a presentation.
        assert_eq!(sniff(b"PK\x03\x04 not really a zip"), None);
    }

    #[test]
    fn garbage_is_unreadable_not_a_panic() {
        assert!(matches!(
            pages(Format::Pdf, b"%PDF-1.4 truncated"),
            Err(DocError::Unreadable(_))
        ));
        assert!(matches!(
            pages(Format::Pptx, b"PK\x03\x04 nope"),
            Err(DocError::Unreadable(_))
        ));
    }
}
