//! Closed response classification, independent transforms, and HTTP serialization.

use crate::{compress_brotli, CONTENT_ENCODING_BROTLI};
use std::borrow::Cow;
use std::fmt::Write as _;
use std::io::{self, Write};

#[derive(Debug, Clone, PartialEq, Eq)]
enum ResponseKind {
    Metadata { encoding: Option<&'static str> },
    Artifact { etag: String },
    Text,
}

#[derive(Debug, Clone)]
pub(super) struct HttpResponse<'a> {
    status: u16,
    reason: &'static str,
    content_type: &'static str,
    body: Cow<'a, [u8]>,
    kind: ResponseKind,
    content_range: Option<String>,
}

impl<'a> HttpResponse<'a> {
    pub fn metadata(content_type: &'static str, body: Vec<u8>) -> Self {
        Self {
            status: 200,
            reason: "OK",
            content_type,
            body: Cow::Owned(body),
            kind: ResponseKind::Metadata { encoding: None },
            content_range: None,
        }
    }

    pub fn artifact(bytes: &'a [u8], digest: &str) -> Self {
        Self {
            status: 200,
            reason: "OK",
            content_type: "application/octet-stream",
            body: Cow::Borrowed(bytes),
            kind: ResponseKind::Artifact {
                etag: format!("\"{digest}\""),
            },
            content_range: None,
        }
    }

    pub fn text(status: u16, reason: &'static str, text: &str) -> Self {
        Self {
            status,
            reason,
            content_type: "text/plain; charset=utf-8",
            body: Cow::Owned(text.as_bytes().to_vec()),
            kind: ResponseKind::Text,
            content_range: None,
        }
    }

    /// Only metadata can be compressed; artifacts can never gain an encoding.
    pub fn compress(mut self, accept_encoding: Option<&str>) -> io::Result<Self> {
        if let ResponseKind::Metadata { encoding } = &mut self.kind {
            if encoding.is_none() && accepts_brotli(accept_encoding) {
                self.body = Cow::Owned(compress_brotli(&self.body)?);
                *encoding = Some(CONTENT_ENCODING_BROTLI);
            }
        }
        Ok(self)
    }

    /// Range selection borrows CAS bytes; it never copies a large artifact slice.
    pub fn select_range(mut self, range: Option<&str>, if_range: Option<&str>) -> Self {
        let ResponseKind::Artifact { etag } = &self.kind else {
            return self;
        };
        let range = range
            .filter(|_| if_range.is_none_or(|validator| validator == etag || validator == "*"));
        let Some(range) = range else {
            return self;
        };
        let length = self.body.len();
        if let Some((start, end)) = byte_range(range, length) {
            self.status = 206;
            self.reason = "Partial Content";
            self.content_range = Some(format!("bytes {start}-{end}/{length}"));
            self.body = match self.body {
                Cow::Borrowed(bytes) => Cow::Borrowed(&bytes[start..=end]),
                Cow::Owned(bytes) => Cow::Owned(bytes[start..=end].to_vec()),
            };
        } else {
            self = Self::text(416, "Range Not Satisfiable", "invalid byte range\n");
            self.content_range = Some(format!("bytes */{length}"));
        }
        self
    }

    pub fn write_to(&self, writer: &mut impl Write, head_only: bool) -> io::Result<()> {
        let mut header = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
            self.status,
            self.reason,
            self.content_type,
            self.body.len()
        );
        // Formatting primitive/string fields into String is infallible, unlike socket I/O.
        match &self.kind {
            ResponseKind::Metadata { encoding } => {
                header.push_str("Vary: Accept-Encoding\r\n");
                if let Some(encoding) = encoding {
                    let _ = write!(header, "Content-Encoding: {encoding}\r\n");
                }
            }
            ResponseKind::Artifact { etag } => {
                let _ = write!(header, "Accept-Ranges: bytes\r\nETag: {etag}\r\n");
            }
            ResponseKind::Text => {}
        }
        if let Some(range) = &self.content_range {
            let _ = write!(header, "Content-Range: {range}\r\n");
        }
        header.push_str("\r\n");
        writer.write_all(header.as_bytes())?;
        if !head_only {
            writer.write_all(&self.body)?;
        }
        writer.flush()
    }
}

fn accepts_brotli(header: Option<&str>) -> bool {
    let Some(header) = header else {
        return false;
    };
    let mut wildcard_quality = None;
    for encoding in header.split(',') {
        let mut parts = encoding.trim().split(';');
        let name = parts.next().unwrap_or_default().trim();
        let quality = parts
            .find_map(|parameter| parameter.trim().strip_prefix("q="))
            .map_or(1.0, |value| {
                value
                    .parse::<f32>()
                    .ok()
                    .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
                    .unwrap_or(0.0)
            });
        if name.eq_ignore_ascii_case("br") {
            return quality > 0.0;
        }
        if name == "*" {
            wildcard_quality = Some(quality);
        }
    }
    wildcard_quality.is_some_and(|quality| quality > 0.0)
}

fn byte_range(range: &str, length: usize) -> Option<(usize, usize)> {
    let (unit, range) = range.split_once('=')?;
    if !unit.eq_ignore_ascii_case("bytes") || range.contains(',') || length == 0 {
        return None;
    }
    let (start, end) = range.split_once('-')?;
    let length = u64::try_from(length).ok()?;
    let (start, end) = if start.is_empty() {
        let suffix = end.parse::<u64>().ok()?;
        if suffix == 0 {
            return None;
        }
        (length.saturating_sub(suffix), length - 1)
    } else {
        let start = start.parse::<u64>().ok()?;
        let end = if end.is_empty() {
            length - 1
        } else {
            end.parse::<u64>().ok()?.min(length - 1)
        };
        (start, end)
    };
    if start > end || start >= length {
        return None;
    }
    Some((usize::try_from(start).ok()?, usize::try_from(end).ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transforms_are_disjoint_and_wire_serialization_is_independent() {
        let metadata = HttpResponse::metadata("application/json", b"{}".to_vec())
            .compress(Some("br"))
            .unwrap()
            .select_range(Some("bytes=0-0"), None);
        assert_eq!(metadata.status, 200);
        assert!(matches!(
            metadata.kind,
            ResponseKind::Metadata {
                encoding: Some("br")
            }
        ));
        assert_eq!(crate::decompress_brotli(&metadata.body).unwrap(), b"{}");
        let artifact = HttpResponse::artifact(b"012345", "digest")
            .compress(Some("br"))
            .unwrap()
            .select_range(Some("bytes=1-3"), Some("\"digest\""));
        assert_eq!(artifact.status, 206);
        assert!(matches!(artifact.body, Cow::Borrowed(b"123")));
        let mut head = Vec::new();
        artifact.write_to(&mut head, true).unwrap();
        assert!(head.ends_with(b"\r\n\r\n"));
        assert!(String::from_utf8(head)
            .unwrap()
            .contains("Content-Length: 3\r\n"));
    }

    #[test]
    fn range_errors_and_encoding_quality_are_fail_closed() {
        for range in ["bytes=99-", "bytes=-0", "bytes=0-1,3-4", "other=0-1"] {
            assert_eq!(
                HttpResponse::artifact(b"abc", "d")
                    .select_range(Some(range), None)
                    .status,
                416
            );
        }
        assert_eq!(
            HttpResponse::artifact(b"abc", "d")
                .select_range(Some("bytes=0-1"), Some("\"other\""))
                .status,
            200
        );
        for header in [
            "br;q=0, *;q=1",
            "br;q=NaN",
            "br;q=9",
            "br;q=invalid",
            "gzip",
        ] {
            assert!(!accepts_brotli(Some(header)));
        }
        assert!(accepts_brotli(Some("gzip, *;q=0.5")));
    }

    #[test]
    fn wire_writer_propagates_io_failure() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "fixture"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert_eq!(
            HttpResponse::text(403, "Forbidden", "closed")
                .write_to(&mut Broken, false)
                .unwrap_err()
                .kind(),
            io::ErrorKind::BrokenPipe
        );
    }
}
