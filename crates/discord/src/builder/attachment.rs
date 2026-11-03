//! Builder for attachment upload metadata and the `multipart/form-data`
//! encoder used by the HTTP layer.
//!
//! Endpoints that accept files use a `multipart/form-data` body with the
//! non-file parameters collected into a `payload_json` part and the files in
//! uniquely named `files[n]` parts (`docs/reference.md`, Uploading Files). The
//! metadata for the files is carried inside the JSON payload as the documented
//! Attachment Request Structure array (`docs/reference.md`, Editing Message
//! Attachments; `docs/message.md`, Attachment Request Structure): for a new
//! attachment the `id` must match the `n` in `files[n]`.
//!
//! The encoder is written from the documented example bodies and RFC 7578:
//! CRLF line endings, a `Content-Disposition` subpart header with a unique
//! `name` (and `filename` for files) and a closing `--boundary--` line. No
//! third-party multipart dependency is used; the boundary is generated from the
//! wall clock and a process-local counter, so it is unique enough for request
//! framing and never needs a random-number dependency.
//!
//! Filenames are inserted into a quoted header parameter, so
//! [`AttachmentBuilder::new`] strips path components and control characters
//! and percent-encodes the `"` that could terminate the quoted string. A
//! filename that sanitises to nothing becomes `file`, because the header
//! requires a non-empty parameter.
//!
//! Reference: `docs/reference.md` (Uploading Files, Editing Message
//! Attachments) and `docs/message.md` (Attachment Object, Attachment Request
//! Structure).

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value};

use super::limits;

/// Maximum characters in an attachment description.
///
/// Reference: `docs/message.md` (Attachment Structure) — "description (alt
/// text) for the file (max 1024 characters)".
const ATTACHMENT_DESCRIPTION: usize = 1024;

/// One file to upload and its documented Attachment Request metadata.
///
/// ```
/// use discord::builder::attachment::AttachmentBuilder;
///
/// let file = AttachmentBuilder::new("screenshot.png", [1_u8, 2, 3])
///     .with_description("The dashboard");
///
/// assert_eq!(file.filename(), "screenshot.png");
/// assert_eq!(file.description(), Some("The dashboard"));
/// assert_eq!(file.data(), &[1, 2, 3]);
/// ```
///
/// Reference: `docs/message.md` (Attachment Request Structure).
#[derive(Clone, PartialEq, Eq)]
pub struct AttachmentBuilder {
    /// The sanitised filename sent in the `files[n]` part.
    filename: String,
    /// The optional alt text, clipped to 1024 characters on serialisation.
    description: Option<String>,
    /// The file contents.
    data: Vec<u8>,
}

impl AttachmentBuilder {
    /// Creates an upload for `data` named `filename`.
    ///
    /// The filename is sanitised before it is put in the multipart
    /// `Content-Disposition` header: any directory components are removed,
    /// control characters (including CR and LF) are stripped, and `"` is
    /// percent-encoded so a caller-controlled name cannot inject header
    /// fields. The sanitised name is what [`Self::filename`] returns and what is
    /// sent in the JSON `attachments` metadata, so the two always match.
    ///
    /// Reference: `docs/reference.md` (Uploading Files) — the `files[n]` parts
    /// need a `filename` in their `Content-Disposition` header.
    #[must_use]
    pub fn new(filename: impl Into<String>, data: impl AsRef<[u8]>) -> Self {
        Self {
            filename: sanitise_filename(&filename.into()),
            description: None,
            data: data.as_ref().to_vec(),
        }
    }

    /// Sets the attachment's description (alt text), clipped to 1024
    /// characters in the metadata.
    ///
    /// Reference: `docs/message.md` (Attachment Request Structure,
    /// `description`).
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Returns the sanitised filename sent in the multipart header and JSON
    /// metadata.
    #[must_use]
    pub fn filename(&self) -> &str {
        &self.filename
    }

    /// Returns the attachment description, when one was set.
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Returns the file contents.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Returns the size of the file in bytes.
    #[must_use]
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// Builds the documented Attachment Request Structure for this file.
    ///
    /// `id` must be the `n` of the `files[n]` part the metadata describes
    /// (`docs/message.md`, Attachment Request Structure; `docs/reference.md`,
    /// Uploading Files). The `filename` is included so the client can reference
    /// the upload in an embed with `attachment://filename`
    /// (`docs/reference.md`, Using Attachments within Embeds), and the
    /// description is omitted when it was not set.
    #[must_use]
    pub fn to_request_json(&self, id: usize) -> Value {
        let mut object = Map::new();
        object.insert("id".to_owned(), Value::from(id));
        object.insert("filename".to_owned(), Value::String(self.filename.clone()));

        if let Some(description) = &self.description {
            object.insert(
                "description".to_owned(),
                Value::String(limits::truncate(description, ATTACHMENT_DESCRIPTION)),
            );
        }

        Value::Object(object)
    }
}

impl fmt::Debug for AttachmentBuilder {
    /// Formats the metadata without dumping the file bytes.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AttachmentBuilder")
            .field("filename", &self.filename)
            .field("description", &self.description)
            .field("size", &self.data.len())
            .finish()
    }
}

/// Builds the documented `attachments` metadata array for `files`.
///
/// The array is the Attachment Request Structure list used by Create Message,
/// Edit Message and the interaction message callbacks
/// (`docs/message.md`, Attachment Request Structure). Each entry's `id` is its
/// index in `files`, matching the `files[n]` part the encoder writes.
#[must_use]
pub fn attachment_request_metadata(files: &[AttachmentBuilder]) -> Value {
    Value::Array(
        files
            .iter()
            .enumerate()
            .map(|(index, file)| file.to_request_json(index))
            .collect(),
    )
}

/// A serialised `multipart/form-data` request body.
///
/// The body is owned bytes, so a retried request can rebuild the same body
/// without re-reading the files.
#[derive(Clone, PartialEq, Eq)]
pub struct MultipartBody {
    /// The generated form boundary.
    boundary: String,
    /// The complete request body, including the closing boundary line.
    body: Vec<u8>,
}

impl MultipartBody {
    /// Returns the form boundary (without the leading `--`).
    #[must_use]
    pub fn boundary(&self) -> &str {
        &self.boundary
    }

    /// Returns the value for the request's `Content-Type` header:
    /// `multipart/form-data; boundary=<boundary>`.
    ///
    /// Reference: `docs/reference.md` (Uploading Files) — upload endpoints use
    /// a `multipart/form-data` request body.
    #[must_use]
    pub fn content_type(&self) -> String {
        format!("multipart/form-data; boundary={}", self.boundary)
    }

    /// Returns the request body bytes.
    #[must_use]
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// Returns the request body bytes, consuming the container.
    #[must_use]
    pub fn into_body(self) -> Vec<u8> {
        self.body
    }
}

impl fmt::Debug for MultipartBody {
    /// Formats the form framing without dumping the file bytes.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MultipartBody")
            .field("boundary", &self.boundary)
            .field("body_len", &self.body.len())
            .finish()
    }
}

/// Encodes `payload_json` and `files` as a documented `multipart/form-data`
/// request body.
///
/// The JSON payload becomes the `payload_json` part (with
/// `Content-Type: application/json`, as in the documented example body) and
/// each file becomes a `files[n]` part with a `filename` and a `Content-Type`
/// derived from the file extension (`docs/reference.md`, Uploading Files). The
/// JSON payload is expected to already contain the matching `attachments`
/// metadata array, for example from [`attachment_request_metadata`].
///
/// Reference: `docs/reference.md` (Uploading Files) and RFC 7578.
#[must_use]
pub fn encode_multipart(payload_json: &Value, files: &[AttachmentBuilder]) -> MultipartBody {
    let boundary = generate_boundary();
    let body = encode_multipart_with_boundary(&boundary, payload_json, files);

    MultipartBody { boundary, body }
}

/// Encodes the body with a caller-chosen boundary, so tests can assert the
/// exact bytes.
fn encode_multipart_with_boundary(
    boundary: &str,
    payload_json: &Value,
    files: &[AttachmentBuilder],
) -> Vec<u8> {
    let mut body = Vec::new();

    // `Value` always serialises; the fallback keeps the encoder infallible
    // without an `unwrap`.
    let payload = serde_json::to_vec(payload_json).unwrap_or_else(|_| b"null".to_vec());
    push_part(
        &mut body,
        boundary,
        "payload_json",
        None,
        "application/json",
        &payload,
    );

    for (index, file) in files.iter().enumerate() {
        push_part(
            &mut body,
            boundary,
            &format!("files[{index}]"),
            Some(&file.filename),
            media_type(&file.filename),
            &file.data,
        );
    }

    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    body
}

/// Appends one multipart part: the boundary line, the part headers, a blank
/// line, the part body and the CRLF that precedes the next boundary.
fn push_part(
    body: &mut Vec<u8>,
    boundary: &str,
    name: &str,
    filename: Option<&str>,
    content_type: &str,
    data: &[u8],
) {
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());

    match filename {
        Some(filename) => body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"")
                .as_bytes(),
        ),
        None => body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{name}\"").as_bytes(),
        ),
    }
    body.extend_from_slice(b"\r\n");

    body.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(data);
    body.extend_from_slice(b"\r\n");
}

/// Generates a unique-enough multipart boundary without a random-number
/// dependency: the current wall clock in nanoseconds plus a process-local
/// counter, in characters RFC 2046 allows in a boundary.
fn generate_boundary() -> String {
    /// Distinguishes boundaries generated in the same nanosecond.
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);

    format!("----discord-{nanos:x}-{counter:x}")
}

/// Returns the media type sent for a file part, based on its extension.
///
/// The media type of a part is informational for Discord, which detects the
/// type from the contents; the common image types documented for embeds
/// (`docs/reference.md`, Using Attachments within Embeds) are mapped so the
/// request matches the documented example, and everything else is sent as
/// `application/octet-stream`.
fn media_type(filename: &str) -> &'static str {
    let extension = filename
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase());

    match extension.as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("json") => "application/json",
        Some("txt") => "text/plain",
        _ => "application/octet-stream",
    }
}

/// Sanitises a caller-controlled filename for a quoted
/// `Content-Disposition` header parameter.
///
/// Directory components are removed (so a path never leaks into the header),
/// control characters including CR and LF are stripped (so no header can be
/// injected), `"` is percent-encoded (so the quoted parameter cannot be
/// terminated early) and an empty result becomes `file`.
fn sanitise_filename(filename: &str) -> String {
    let basename = filename
        .rsplit(['/', '\\'])
        .next()
        .filter(|basename| !basename.is_empty())
        .unwrap_or("file");

    let mut sanitised = String::with_capacity(basename.len());

    for character in basename.chars() {
        match character {
            '"' => sanitised.push_str("%22"),
            character if character.is_control() => {}
            character => sanitised.push(character),
        }
    }

    if sanitised.is_empty() {
        "file".to_owned()
    } else {
        sanitised
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        ATTACHMENT_DESCRIPTION, AttachmentBuilder, attachment_request_metadata,
        encode_multipart_with_boundary, generate_boundary, media_type, sanitise_filename,
    };

    /// Builds the expected body bytes for one payload part and one file part,
    /// spelled out the way RFC 7578 and `docs/reference.md` (Uploading Files)
    /// document them.
    fn expected_body(boundary: &str) -> String {
        format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"payload_json\"\r\n\
             Content-Type: application/json\r\n\
             \r\n\
             {{\"content\":\"Hello, World!\"}}\r\n\
             --{boundary}\r\n\
             Content-Disposition: form-data; name=\"files[0]\"; filename=\"myfilename.png\"\r\n\
             Content-Type: image/png\r\n\
             \r\n\
             [image bytes]\r\n\
             --{boundary}--\r\n"
        )
    }

    #[test]
    fn encoder_matches_the_documented_body_byte_for_byte() {
        let payload = json!({"content": "Hello, World!"});
        let files = [AttachmentBuilder::new("myfilename.png", b"[image bytes]")];

        let body = encode_multipart_with_boundary("boundary", &payload, &files);

        assert_eq!(body, expected_body("boundary").into_bytes());
    }

    #[test]
    fn encoder_writes_a_non_ascii_filename_as_utf8() {
        let payload =
            json!({"content": "Résumé", "attachments": [{"id": 0, "filename": "résumé 🎉.png"}]});
        let files = [AttachmentBuilder::new("résumé 🎉.png", b"png bytes")];

        let body = encode_multipart_with_boundary("boundary", &payload, &files);
        let expected = "--boundary\r\n\
             Content-Disposition: form-data; name=\"payload_json\"\r\n\
             Content-Type: application/json\r\n\
             \r\n\
             {\"attachments\":[{\"filename\":\"résumé 🎉.png\",\"id\":0}],\"content\":\"Résumé\"}\r\n\
             --boundary\r\n\
             Content-Disposition: form-data; name=\"files[0]\"; filename=\"résumé 🎉.png\"\r\n\
             Content-Type: image/png\r\n\
             \r\n\
             png bytes\r\n\
             --boundary--\r\n";

        assert_eq!(body, expected.as_bytes());
        // The header parameter round-trips as UTF-8.
        let text =
            String::from_utf8(body).expect("the multipart body is valid UTF-8 with text parts");
        assert!(text.contains("filename=\"résumé 🎉.png\""));
    }

    #[test]
    fn encoder_numbers_multiple_files_by_their_metadata_index() {
        let payload = json!({"attachments": [{"id": 0, "filename": "a.txt"}, {"id": 1, "filename": "b.gif"}]});
        let files = [
            AttachmentBuilder::new("a.txt", b"a"),
            AttachmentBuilder::new("b.gif", b"b"),
        ];

        let body = encode_multipart_with_boundary("b", &payload, &files);
        let text = String::from_utf8(body).expect("text parts");

        assert!(text.contains(r#"name="files[0]"; filename="a.txt""#));
        assert!(text.contains(r#"name="files[1]"; filename="b.gif""#));
        assert!(text.contains("Content-Type: text/plain\r\n"));
        assert!(text.contains("Content-Type: image/gif\r\n"));
        assert!(text.ends_with("--b--\r\n"));
    }

    #[test]
    fn encoder_keeps_binary_file_bytes_untouched() {
        let bytes = [0_u8, 1, 2, 0xff, b'\r', b'\n'];
        let files = [AttachmentBuilder::new("raw.bin", bytes)];

        let body = encode_multipart_with_boundary("b", &json!({}), &files);

        // The raw bytes are copied verbatim between the part headers and the
        // CRLF that precedes the closing boundary.
        let file_header = b"Content-Disposition: form-data; name=\"files[0]\"; filename=\"raw.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n";
        let start = body
            .windows(file_header.len())
            .position(|window| window == file_header)
            .expect("the file part header is present")
            + file_header.len();

        assert_eq!(&body[start..start + bytes.len()], bytes);
        assert_eq!(&body[start + bytes.len()..start + bytes.len() + 2], b"\r\n");
    }

    #[test]
    fn attachment_metadata_matches_the_files_in_order() {
        let files = [
            AttachmentBuilder::new("one.png", b"1"),
            AttachmentBuilder::new("two.txt", b"2").with_description("The second file"),
        ];

        let metadata = attachment_request_metadata(&files);

        assert_eq!(
            metadata,
            json!([
                {"id": 0, "filename": "one.png"},
                {"id": 1, "filename": "two.txt", "description": "The second file"}
            ])
        );
    }

    #[test]
    fn attachment_description_is_clipped_to_the_documented_limit() {
        let file = AttachmentBuilder::new("a.txt", b"").with_description("d".repeat(2_000));

        let metadata = file.to_request_json(0);
        let description = metadata["description"].as_str().expect("description");

        assert_eq!(description.chars().count(), ATTACHMENT_DESCRIPTION);
    }

    #[test]
    fn filenames_cannot_inject_multipart_headers() {
        // A path and header-injection payload: the basename is kept, CR/LF are
        // stripped and the quote that would close the parameter is escaped.
        let injected = "..\\evil\r\nX-Injected: yes\"name.txt";
        let sanitised = sanitise_filename(injected);

        assert_eq!(sanitised, "evilX-Injected: yes%22name.txt");
        assert!(!sanitised.contains(['\r', '\n', '"']));
        assert!(!sanitised.contains('/'));

        let file = AttachmentBuilder::new(injected, b"");
        assert_eq!(file.filename(), sanitised);

        let body = encode_multipart_with_boundary("b", &json!({}), &[file]);
        let text = String::from_utf8(body).expect("text parts");
        assert!(!text.contains("X-Injected: yes\""));
        assert_eq!(text.matches("Content-Disposition:").count(), 2);
    }

    #[test]
    fn empty_and_path_only_filenames_fall_back_to_file() {
        assert_eq!(sanitise_filename(""), "file");
        assert_eq!(sanitise_filename("/"), "file");
        assert_eq!(sanitise_filename("\\"), "file");
        assert_eq!(sanitise_filename("C:\\dir\\"), "file");
        assert_eq!(sanitise_filename("dir/photo.png"), "photo.png");
    }

    #[test]
    fn generated_boundaries_are_unique_and_content_type_is_documented() {
        let first = generate_boundary();
        let second = generate_boundary();

        assert_ne!(first, second);
        assert!(first.starts_with("----discord-"));
        assert!(
            first
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-'),
            "unexpected boundary characters in {first}"
        );

        let form = super::encode_multipart(&json!({"content": "hi"}), &[]);
        let expected_content_type = format!("multipart/form-data; boundary={}", form.boundary());
        let body_len = form.body().len();

        assert_eq!(form.content_type(), expected_content_type);
        assert!(body_len > 0);
        assert_eq!(form.into_body().len(), body_len);
    }

    #[test]
    fn media_types_follow_the_file_extension() {
        assert_eq!(media_type("a.png"), "image/png");
        assert_eq!(media_type("a.JPG"), "image/jpeg");
        assert_eq!(media_type("a.jpeg"), "image/jpeg");
        assert_eq!(media_type("a.gif"), "image/gif");
        assert_eq!(media_type("a.webp"), "image/webp");
        assert_eq!(media_type("a.json"), "application/json");
        assert_eq!(media_type("a.txt"), "text/plain");
        assert_eq!(media_type("a.bin"), "application/octet-stream");
        assert_eq!(media_type("no-extension"), "application/octet-stream");
    }
}
