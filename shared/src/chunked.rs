use std::{fmt::Display, io};

use thiserror::Error;

const SEPARATOR: &[u8] = b"\r\n";

pub trait IntoChunk {
    fn into_chunk(self) -> io::Result<Vec<u8>>;
}

impl IntoChunk for Vec<u8> {
    fn into_chunk(self) -> io::Result<Vec<u8>> {
        Ok(self)
    }
}

impl IntoChunk for String {
    fn into_chunk(self) -> io::Result<Vec<u8>> {
        Ok(self.into_bytes())
    }
}

impl IntoChunk for &str {
    fn into_chunk(self) -> io::Result<Vec<u8>> {
        Ok(self.as_bytes().to_vec())
    }
}

impl IntoChunk for &[u8] {
    fn into_chunk(self) -> io::Result<Vec<u8>> {
        Ok(self.to_vec())
    }
}

impl<T, E> IntoChunk for Result<T, E>
where
    T: IntoChunk,
    E: Display,
{
    fn into_chunk(self) -> io::Result<Vec<u8>> {
        self.map_err(|e| io::Error::other(e.to_string()))?
            .into_chunk()
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Error)]
pub enum ChunkError {
    #[error("incomplete chunk size")]
    IncompleteChunkSize,
    #[error("invalid chunk size")]
    InvalidChunkSize,
    #[error("invalid chunked encoding")]
    InvalidChunkedEncoding,
    #[error("chunk or body too large")]
    TooLarge,
}

pub fn parse_chunk_size(data: &[u8]) -> Result<(usize, usize), ChunkError> {
    let line_end = data
        .windows(SEPARATOR.len())
        .position(|window| window == SEPARATOR)
        .ok_or(ChunkError::IncompleteChunkSize)?;

    let line = &data[..line_end];
    if line.is_empty() {
        return Err(ChunkError::InvalidChunkSize);
    }

    let size_end = line
        .iter()
        .position(|&b| b == b';')
        .unwrap_or(line.len());

    let size_str = std::str::from_utf8(&line[..size_end])
        .map_err(|_| ChunkError::InvalidChunkSize)?
        .trim();

    let size = usize::from_str_radix(size_str, 16)
        .map_err(|_| ChunkError::InvalidChunkSize)?;

    let bytes_consumed = line_end + SEPARATOR.len();
    Ok((size, bytes_consumed))
}

/// Parses trailers headers.
pub fn parse_trailing_headers(data: &[u8]) -> Result<(usize, bool), ChunkError> {
    // Look for empty line (CRLF CRLF) which marks end of trailing headers
    if data.len() < 2 {
        return Ok((0, false));
    }

    // Check if we have just CRLF (no trailing headers)
    if data.starts_with(SEPARATOR) {
        return Ok((SEPARATOR.len(), true));
    }

    // Look for end of trailing headers (CRLF CRLF)
    for i in 0..data.len().saturating_sub(3) {
        if &data[i..i + 4] == b"\r\n\r\n" {
            return Ok((i + 4, true));
        }
    }

    Ok((0, false))
}

pub fn parse_chunked_body(
    data: &[u8],
    body: &mut Vec<u8>,
    max_body_size: usize,
) -> Result<(usize, bool), ChunkError> {
    let mut consumed = 0;
    while consumed < data.len() {
        let chunk_data = &data[consumed..];
        let (chunk_size, size_bytes) = match parse_chunk_size(chunk_data) {
            Ok(result) => result,
            Err(ChunkError::IncompleteChunkSize) => break,
            Err(err) => return Err(err),
        };

        if chunk_size == 0 {
            let (trailing_bytes, complete) = parse_trailing_headers(&chunk_data[size_bytes..])?;
            if !complete {
                break;
            }
            consumed += size_bytes + trailing_bytes;
            return Ok((consumed, true));
        }

        let total_len = size_bytes + chunk_size + 2;
        if chunk_data.len() < total_len {
            break;
        }

        if &chunk_data[size_bytes + chunk_size..total_len] != b"\r\n" {
            return Err(ChunkError::InvalidChunkedEncoding);
        }

        if body.len() + chunk_size > max_body_size {
            return Err(ChunkError::TooLarge);
        }

        body.extend_from_slice(&chunk_data[size_bytes..size_bytes + chunk_size]);
        consumed += total_len;
    }

    Ok((consumed, false))
}

/// Encodes a chunk formatted with hex size and CRLF. If empty, encodes terminal chunk `0\r\n\r\n`.
pub fn encode_chunk(chunk: &[u8]) -> Vec<u8> {
    if chunk.is_empty() {
        return b"0\r\n\r\n".to_vec();
    }
    let mut encoded = Vec::with_capacity(chunk.len() + 16);
    use std::io::Write;
    let _ = write!(&mut encoded, "{:x}\r\n", chunk.len());
    encoded.extend_from_slice(chunk);
    encoded.extend_from_slice(b"\r\n");
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_parse_simple_chunk_size() {
        let data = b"1a\r\n";
        let (size, consumed) = parse_chunk_size(data).expect("Should parse");
        assert_eq!(size, 26); // 0x1a = 26
        assert_eq!(consumed, 4);
    }

    #[test]
    fn should_parse_chunk_size_with_extensions() {
        let data = b"1a;name=value\r\n";
        let (size, consumed) = parse_chunk_size(data).expect("Should parse");
        assert_eq!(size, 26);
        assert_eq!(consumed, 15);
    }

    #[test]
    fn should_error_on_incomplete_chunk_size() {
        let data = b"1a";
        assert!(matches!(
            parse_chunk_size(data),
            Err(ChunkError::IncompleteChunkSize)
        ));
    }

    #[test]
    fn should_parse_large_chunk_size() {
        let data = b"FFFF\r\n";
        let (size, consumed) = parse_chunk_size(data).expect("Should parse");
        assert_eq!(size, 65535);
        assert_eq!(consumed, 6);
    }

    #[test]
    fn should_parse_trailing_headers_empty() {
        let data = b"\r\n";
        let (consumed, complete) = parse_trailing_headers(data).expect("Should parse");
        assert_eq!(consumed, 2);
        assert!(complete);
    }

    #[test]
    fn should_parse_trailing_headers_with_content() {
        let data = b"X-Checksum: abc123\r\n\r\n";
        let (consumed, complete) = parse_trailing_headers(data).expect("Should parse");
        assert_eq!(consumed, 22);
        assert!(complete);
    }

    #[test]
    fn should_handle_incomplete_trailing_headers() {
        let data = b"X-Checksum: abc";
        let (consumed, complete) = parse_trailing_headers(data).expect("Should parse");
        assert_eq!(consumed, 0);
        assert!(!complete);
    }

    #[test]
    fn should_parse_chunked_stream() {
        let mut body = Vec::new();
        let data = b"5\r\nhello\r\n0\r\n\r\n";
        let (consumed, done) = parse_chunked_body(data, &mut body, 1024).expect("Should parse");
        assert_eq!(consumed, data.len());
        assert!(done);
        assert_eq!(&body, b"hello");
    }

    #[test]
    fn should_encode_chunk() {
        assert_eq!(encode_chunk(b"hello"), b"5\r\nhello\r\n");
        assert_eq!(encode_chunk(b""), b"0\r\n\r\n");
    }

    #[test]
    fn should_convert_chunks() {
        assert_eq!(b"hello".to_vec().into_chunk().unwrap(), b"hello");
        assert_eq!("hello".into_chunk().unwrap(), b"hello");
        assert_eq!(
            Ok::<_, ChunkError>(b"hello".to_vec()).into_chunk().unwrap(),
            b"hello"
        );
        assert!(
            Err::<Vec<u8>, _>(ChunkError::TooLarge)
                .into_chunk()
                .is_err()
        );
    }
}
