use std::{
    cmp::min,
    io::{self, Read},
};

use crate::headers::Headers;
#[derive(Debug)]
pub struct Request {
    pub request_line: Option<RequestLine>,
    pub headers: Headers,
    pub body: Vec<u8>,
    parse_state: ParseState,
}
#[derive(Debug)]
pub struct RequestLine {
    pub http_version: String,
    pub request_target: String,
    pub method: String,
}

#[derive(Debug)]
enum ParseState {
    INIT,
    HEADERS,
    BODY,
}

#[derive(Debug, PartialEq)]
pub enum ParseError {
    IncompleteRequestLine,
    InvalidRequestLine,
    UnsupportedHttpVersion,
    IncompleteFieldLine,
    InvalidFieldLine,
    InvalidHeaderKey,
    InvalidContentLengthHeader,
}
const SEPARATOR: &[u8] = b"\r\n";
impl Request {
    pub fn new() -> Request {
        Request {
            headers: Headers::new(),
            body: Vec::new(),
            parse_state: ParseState::INIT,
            request_line: None,
        }
    }
    pub fn parse(&mut self, data: &[u8]) -> Result<usize, ParseError> {
        let mut read = 0;
        loop {
            let curr_data = &data[read..];
            if curr_data.is_empty() {
                return Ok(read);
            }
            match self.parse_state {
                ParseState::INIT => {
                    let result = parse_request_line(curr_data)?;
                    if result.1 == 0 {
                        return Ok(result.1);
                    }
                    self.request_line = Some(result.0);
                    read += result.1;
                    self.parse_state = ParseState::HEADERS
                }
                ParseState::HEADERS => {
                    let result = self.headers.parse(curr_data)?;
                    read += result.0;
                    if result.1 {
                        self.parse_state = ParseState::BODY
                    }
                }
                ParseState::BODY => {
                    let length = self
                        .headers
                        .get("content-length")
                        .ok_or(ParseError::InvalidContentLengthHeader)?
                        .parse::<usize>()
                        .map_err(|_| ParseError::InvalidContentLengthHeader)?;
                    if length == 0 {
                        return Ok(read);
                    }
                    let remaining = min(curr_data.len(), length - self.body.len());
                    self.body.extend_from_slice(&curr_data[..remaining]);
                    read += remaining;
                    if self.body.len() == remaining {
                        return Ok(read);
                    }
                    if remaining == 0 && self.body.len() != length {
                        return Err(ParseError::InvalidContentLengthHeader);
                    }
                }
            }
        }
    }
}
#[derive(Debug)]
pub enum RequestReadError {
    Io(io::Error),
    Parse(ParseError),
    RequestTooLarge,
    UnexexpectedEndOfInput,
}
pub fn request_from_reader<R: Read>(reader: &mut R) -> Result<Request, RequestReadError> {
    let mut request = Request::new();
    let mut buffer = [0_u8; 1024];
    let mut buffer_index = 0;
    loop {
        if buffer_index == buffer.len() {
            return Err(RequestReadError::RequestTooLarge);
        }
        let bytes_read = reader
            .read(&mut buffer[buffer_index..])
            .map_err(RequestReadError::Io)?;
        if bytes_read == 0 {
            return Err(RequestReadError::UnexexpectedEndOfInput);
        }
        let read = request.parse(&buffer);
    }
}

fn parse_request_line(data: &[u8]) -> Result<(RequestLine, usize), ParseError> {
    let idx = data
        .windows(SEPARATOR.len())
        .position(|window| window == SEPARATOR)
        .ok_or(ParseError::IncompleteRequestLine)?;
    let request_line = &data[..idx];
    let read = idx + SEPARATOR.len();
    let mut parts = request_line.split(|&x| x == b' ');

    let method = parts.next().ok_or(ParseError::InvalidRequestLine)?;
    let request_target = parts.next().ok_or(ParseError::InvalidRequestLine)?;
    let http = parts.next().ok_or(ParseError::InvalidRequestLine)?;

    // make sure there is not a 4th part
    if parts.next().is_some() {
        return Err(ParseError::InvalidRequestLine);
    }

    let mut http_parts = http.split(|&x| x == b'/');

    let protocol = http_parts.next().ok_or(ParseError::InvalidRequestLine)?;
    let version = http_parts.next().ok_or(ParseError::InvalidRequestLine)?;

    if protocol != b"HTTP" || version != b"1.1" || http_parts.next().is_some() {
        return Err(ParseError::UnsupportedHttpVersion);
    }

    Ok((
        RequestLine {
            method: std::str::from_utf8(method)
                .map_err(|_| ParseError::InvalidRequestLine)?
                .to_string(),
            request_target: str::from_utf8(request_target)
                .map_err(|_| ParseError::InvalidRequestLine)?
                .to_string(),
            http_version: str::from_utf8(version)
                .map_err(|_| ParseError::InvalidRequestLine)?
                .to_string(),
        },
        read,
    ))
}

// let line = std::str::from_utf8(line_bytes).ok()?;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn should_parse_request_line() {
        let result =
            parse_request_line(b"GET / HTTP/1.1\r\n").expect("Valid request line should pass");

        let request_line = result.0;
        assert_eq!(request_line.http_version, "1.1");
        assert_eq!(request_line.method, "GET");
        assert_eq!(result.1, 16, "expected content length")
    }
    #[test]
    fn should_fail_parse_request_line() {
        let result = parse_request_line(b"GET / HTTP/1.1 foobar\r\n");
        assert!(result.is_err());
        println!("result {:#?}", result);
    }
}
