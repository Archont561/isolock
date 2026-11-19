//! HTTP parsing with an absolute header deadline and a fixed byte ceiling.

use std::collections::HashMap;
use std::io::{self, Read};
use std::net::TcpStream;
use std::time::{Duration, Instant};

const MAX_HEADER_BYTES: usize = 64 * 1024;

pub(super) struct HttpRequest {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
}

pub(super) fn read_request(
    stream: &mut TcpStream,
    timeout: Duration,
) -> io::Result<Option<HttpRequest>> {
    let mut bytes = Vec::with_capacity(2048);
    let mut chunk = [0_u8; 1024];
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "HTTP header deadline exceeded",
            ));
        }
        stream.set_read_timeout(Some(remaining))?;
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return if bytes.is_empty() {
                Ok(None)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "incomplete request",
                ))
            };
        }
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            if position + 4 > MAX_HEADER_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "HTTP headers exceed the configured limit",
                ));
            }
            bytes.truncate(position + 4);
            break;
        }
        if bytes.len() > MAX_HEADER_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "HTTP headers exceed the configured limit",
            ));
        }
    }
    parse_request(&bytes)
}

pub(super) fn parse_request(bytes: &[u8]) -> io::Result<Option<HttpRequest>> {
    let request = std::str::from_utf8(bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "HTTP headers are not UTF-8"))?;
    let mut lines = request.split("\r\n");
    let Some(request_line) = lines.next() else {
        return Ok(None);
    };
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or_default();
    let path = request_parts.next().unwrap_or_default();
    let version = request_parts.next().unwrap_or_default();
    if method.is_empty()
        || path.is_empty()
        || !matches!(version, "HTTP/1.0" | "HTTP/1.1")
        || request_parts.next().is_some()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid request line",
        ));
    }
    let mut headers = HashMap::new();
    for line in lines.take_while(|line| !line.is_empty()) {
        let Some((name, value)) = line.split_once(':') else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid HTTP header",
            ));
        };
        if name.is_empty() || !name.bytes().all(is_header_name_byte) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid HTTP header name",
            ));
        }
        let key = name.to_ascii_lowercase();
        let value = value.trim();
        headers
            .entry(key)
            .and_modify(|existing: &mut String| {
                existing.push_str(", ");
                existing.push_str(value);
            })
            .or_insert_with(|| value.to_owned());
    }
    Ok(Some(HttpRequest {
        method: method.to_owned(),
        path: path.to_owned(),
        headers,
    }))
}

fn is_header_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::{Shutdown, TcpListener};
    use std::thread;

    fn read_wire(bytes: Vec<u8>) -> io::Result<Option<HttpRequest>> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let client = thread::spawn(move || {
            let mut stream = TcpStream::connect(address).unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let _ = stream.write_all(&bytes);
            let _ = stream.shutdown(Shutdown::Write);
        });
        let (mut stream, _) = listener.accept().unwrap();
        let result = read_request(&mut stream, Duration::from_secs(2));
        client.join().unwrap();
        result
    }

    #[test]
    fn parser_merges_header_names_and_rejects_invalid_syntax() {
        let request = parse_request(b"HEAD /cargo/config.json HTTP/1.1\r\nAccept-Encoding: gzip\r\naccept-encoding: br\r\n\r\n").unwrap().unwrap();
        assert_eq!(request.method, "HEAD");
        assert_eq!(request.path, "/cargo/config.json");
        assert_eq!(request.headers["accept-encoding"], "gzip, br");
        for invalid in [
            b"GET / HTTP/9\r\n\r\n".as_slice(),
            b"GET / HTTP/1.1 extra\r\n\r\n",
            b"GET / HTTP/1.1\r\nBad Header: value\r\n\r\n",
            b"GET / HTTP/1.1\r\ninvalid-header\r\n\r\n",
            b"\xff",
        ] {
            assert!(
                matches!(parse_request(invalid), Err(error) if error.kind() == io::ErrorKind::InvalidData)
            );
        }
    }

    #[test]
    fn header_ceiling_is_inclusive_and_incomplete_requests_are_not_accepted() {
        let mut exact = b"GET / HTTP/1.1\r\nX-Pad: ".to_vec();
        exact.resize(MAX_HEADER_BYTES - 4, b'x');
        exact.extend_from_slice(b"\r\n\r\n");
        assert!(read_wire(exact.clone()).unwrap().is_some());
        exact.insert(MAX_HEADER_BYTES - 4, b'x');
        assert!(
            matches!(read_wire(exact), Err(error) if error.kind() == io::ErrorKind::InvalidData)
        );
        assert!(read_wire(Vec::new()).unwrap().is_none());
        assert!(
            matches!(read_wire(b"GET / HTTP/1.1\r\n".to_vec()), Err(error) if error.kind() == io::ErrorKind::UnexpectedEof)
        );
    }
}
