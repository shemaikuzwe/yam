use std::io::Read;

use flate2::read::{DeflateDecoder, GzDecoder, ZlibDecoder};

use crate::error::Error;

pub(crate) fn decode(headers: &mut Vec<(String, String)>, body: Vec<u8>) -> Result<Vec<u8>, Error> {
    let encodings: Vec<String> = headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("content-encoding"))
        .flat_map(|(_, value)| value.split(','))
        .map(|token| token.trim().to_ascii_lowercase())
        .filter(|token| !token.is_empty() && token != "identity")
        .collect();

    if encodings.is_empty() || body.is_empty() {
        return Ok(body);
    }

    let mut decoded = body;
    for encoding in encodings.iter().rev() {
        decoded = match encoding.as_str() {
            "gzip" | "x-gzip" => read_all(GzDecoder::new(&decoded[..]), encoding)?,
            // Some servers send raw deflate despite RFC 2616 specifying zlib.
            "deflate" => read_all(ZlibDecoder::new(&decoded[..]), encoding)
                .or_else(|_| read_all(DeflateDecoder::new(&decoded[..]), encoding))?,
            other => {
                return Err(Error::InvalidResponse(format!(
                    "unsupported content-encoding: {other}"
                )));
            }
        };
    }

    headers.retain(|(name, _)| {
        !name.eq_ignore_ascii_case("content-encoding")
            && !name.eq_ignore_ascii_case("content-length")
    });
    Ok(decoded)
}

fn read_all(mut reader: impl Read, encoding: &str) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    reader
        .read_to_end(&mut out)
        .map_err(|e| Error::InvalidResponse(format!("failed to decode {encoding} body: {e}")))?;
    Ok(out)
}
