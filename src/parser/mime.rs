//! MIME message parsing: body extraction, attachment listing, HTML-to-text conversion.

use mail_parser::MessageParser;

use crate::error::{MboxError, Result};
use crate::model::attachment::AttachmentMeta;
use crate::model::mail::MailBody;

/// Parse a complete raw message (headers + body) and extract its content.
///
/// Uses `mail-parser` internally, with extra fallbacks for malformed messages.
pub fn parse_message_body(raw_message: &[u8]) -> Result<MailBody> {
    // Strip the leading "From " separator line if present
    let message_bytes = skip_from_line(raw_message);

    let parser = MessageParser::default();
    let parsed = parser.parse(message_bytes);

    match parsed {
        Some(msg) => {
            let raw_headers = extract_raw_headers(message_bytes);

            let text = msg
                .body_text(0)
                .map(|s| s.into_owned())
                .or_else(|| msg.body_html(0).map(|html| html_to_text(&html, 80)));

            let html = msg.body_html(0).map(|s| s.into_owned());

            let attachments = list_attachments_from_parsed(&msg);

            Ok(MailBody {
                text,
                html,
                raw_headers,
                attachments,
            })
        }
        None => {
            // Fallback: return what we can
            let raw_headers = extract_raw_headers(message_bytes);
            let body_text = extract_body_fallback(message_bytes);
            Ok(MailBody {
                text: Some(body_text),
                html: None,
                raw_headers,
                attachments: Vec::new(),
            })
        }
    }
}

/// List attachment metadata from a raw message WITHOUT decoding their content.
pub fn list_attachments(raw_message: &[u8]) -> Result<Vec<AttachmentMeta>> {
    let message_bytes = skip_from_line(raw_message);
    let parser = MessageParser::default();
    match parser.parse(message_bytes) {
        Some(msg) => Ok(list_attachments_from_parsed(&msg)),
        None => Ok(Vec::new()),
    }
}

/// Decode and extract the binary content of a specific attachment.
pub fn extract_attachment(raw_message: &[u8], attachment: &AttachmentMeta) -> Result<Vec<u8>> {
    let message_bytes = skip_from_line(raw_message);
    let parser = MessageParser::default();
    let msg = parser.parse(message_bytes).ok_or_else(|| {
        MboxError::MimeError("Failed to parse message for attachment extraction".into())
    })?;

    use mail_parser::MimeHeaders;
    // Find the attachment by filename match
    for part in msg.attachments() {
        let name = part.attachment_name().unwrap_or("").to_string();

        if name == attachment.filename || attachment.filename.is_empty() {
            return Ok(part.contents().to_vec());
        }
    }

    Err(MboxError::MimeError(format!(
        "Attachment '{}' not found in message",
        attachment.filename
    )))
}

/// Build attachment metadata from a parsed `mail_parser::Message`.
fn list_attachments_from_parsed(msg: &mail_parser::Message<'_>) -> Vec<AttachmentMeta> {
    use mail_parser::MimeHeaders;

    let mut result = Vec::new();

    for (idx, part) in msg.attachments().enumerate() {
        let filename = part
            .attachment_name()
            .map(String::from)
            .unwrap_or_else(|| format!("attachment_{idx}"));

        let content_type = part
            .content_type()
            .map(|ct: &mail_parser::ContentType| {
                let main = ct.ctype();
                match ct.subtype() {
                    Some(sub) => format!("{main}/{sub}"),
                    None => main.to_string(),
                }
            })
            .unwrap_or_else(|| "application/octet-stream".to_string());

        let is_inline = part
            .content_disposition()
            .map(|d: &mail_parser::ContentType| d.ctype() == "inline")
            .unwrap_or(false);

        result.push(AttachmentMeta {
            filename,
            content_type,
            size: part.contents().len() as u64,
            encoding: String::new(), // mail-parser already decoded it
            content_id: None,
            is_inline,
            content_offset: 0,
            content_length: part.contents().len() as u64,
        });
    }

    result
}

/// Skip the `From ` separator line at the start of MBOX messages.
fn skip_from_line(data: &[u8]) -> &[u8] {
    // Handle BOM
    let data = if data.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &data[3..]
    } else {
        data
    };

    if data.starts_with(b"From ") {
        // Find end of line
        if let Some(pos) = data.iter().position(|&b| b == b'\n') {
            return &data[pos + 1..];
        }
    }
    data
}

/// Extract the raw headers as a string (everything before the first blank line).
fn extract_raw_headers(data: &[u8]) -> String {
    let text = String::from_utf8_lossy(data);
    // Find double newline (end of headers)
    if let Some(pos) = text.find("\n\n") {
        text[..pos].to_string()
    } else if let Some(pos) = text.find("\r\n\r\n") {
        text[..pos].to_string()
    } else {
        text.to_string()
    }
}

/// Fallback body extraction when `mail-parser` cannot parse the message.
fn extract_body_fallback(data: &[u8]) -> String {
    let text = String::from_utf8_lossy(data);
    // Everything after the first blank line is the body
    if let Some(pos) = text.find("\n\n") {
        text[pos + 2..].to_string()
    } else if let Some(pos) = text.find("\r\n\r\n") {
        text[pos + 4..].to_string()
    } else {
        String::new()
    }
}

/// Convert HTML to plain text for terminal display.
///
/// Uses the `html2text` crate for proper rendering of tables, lists,
/// headings, links and nested structures. `width` is the target line
/// width in columns; pass the actual terminal width when available, or
/// a sensible default (e.g. 100) otherwise.
pub fn html_to_text(html: &str, width: usize) -> String {
    let width = width.max(20);
    html2text::from_read(html.as_bytes(), width)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skip_from_line() {
        let data = b"From user@example.com Thu Jan 01 00:00:00 2024\nSubject: Test\n\nBody\n";
        let result = skip_from_line(data);
        assert!(result.starts_with(b"Subject:"));
    }

    #[test]
    fn test_skip_from_line_no_from() {
        let data = b"Subject: Test\n\nBody\n";
        let result = skip_from_line(data);
        assert_eq!(result, data);
    }

    #[test]
    fn test_html_to_text_basic() {
        let html = "<p>Hello <b>world</b></p><p>Second paragraph</p>";
        let text = html_to_text(html, 80);
        assert!(text.contains("Hello world"));
        assert!(text.contains("Second paragraph"));
    }

    #[test]
    fn test_html_to_text_entities() {
        let html = "Tom &amp; Jerry &lt;3&gt;";
        let text = html_to_text(html, 80);
        assert_eq!(text.trim(), "Tom & Jerry <3>");
    }

    #[test]
    fn test_html_to_text_removes_scripts() {
        let html = "Before<script>alert('xss')</script>After";
        let text = html_to_text(html, 80);
        assert_eq!(text.trim(), "BeforeAfter");
    }

    #[test]
    fn test_html_to_text_tables() {
        let html = "<table><tr><th>Col1</th><th>Col2</th></tr>\
                    <tr><td>A</td><td>B</td></tr></table>";
        let text = html_to_text(html, 60);
        assert!(text.contains("Col1"));
        assert!(text.contains("Col2"));
        assert!(text.contains('A'));
        assert!(text.contains('B'));
    }

    #[test]
    fn test_html_to_text_links() {
        let html = r#"Click <a href="https://example.com">here</a>"#;
        let text = html_to_text(html, 80);
        assert!(text.contains("here"));
        assert!(text.contains("example.com"));
    }

    #[test]
    fn test_extract_raw_headers() {
        let data = b"From: alice@example.com\nSubject: Hi\n\nBody here\n";
        let headers = extract_raw_headers(data);
        assert!(headers.contains("From: alice@example.com"));
        assert!(headers.contains("Subject: Hi"));
        assert!(!headers.contains("Body here"));
    }
}
