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

#[derive(Debug, PartialEq)]
enum ParseState {
    INIT,
    HEADERS,
    BODY,
    DONE,
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
    fn done(&self) -> bool {
        self.parse_state == ParseState::DONE
    }
    fn has_body(&self) -> bool {
        let content_length = self.headers.get("content-length");
        if content_length.is_none() {
            return false;
        }
        let content_length = content_length.unwrap().parse::<usize>().unwrap_or(0);
        if content_length == 0 {
            return false;
        }
        true
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
                    let result = match parse_request_line(curr_data) {
                        Ok(result) => result,
                        Err(ParseError::IncompleteRequestLine) => return Ok(read),
                        Err(err) => return Err(err),
                    };
                    if result.1 == 0 {
                        return Ok(result.1);
                    }
                    self.request_line = Some(result.0);
                    read += result.1;
                    self.parse_state = ParseState::HEADERS
                }
                ParseState::HEADERS => {
                    let result = self.headers.parse(curr_data)?;
                    if result.0 == 0 {
                        return Ok(read);
                    }
                    read += result.0;
                    if result.1 {
                        if self.has_body() {
                            self.parse_state = ParseState::BODY
                        } else {
                            self.parse_state = ParseState::DONE
                        }
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
                        self.parse_state = ParseState::DONE;
                        return Ok(read);
                    }
                    let remaining = min(curr_data.len(), length - self.body.len());
                    self.body.extend_from_slice(&curr_data[..remaining]);
                    read += remaining;
                    if self.body.len() == length {
                        self.parse_state = ParseState::DONE;
                        return Ok(read);
                    }
                    if remaining == 0 && self.body.len() != length {
                        return Err(ParseError::InvalidContentLengthHeader);
                    }
                }
                ParseState::DONE => {
                    return Ok(read);
                }
            }
        }
    }
}
#[derive(Debug)]
pub enum RequestError {
    Io(io::Error),
    Parse(ParseError),
    RequestTooLarge,
    UnexexpectedEndOfInput,
}
pub struct RequestReader<R> {
    reader: R,
    buffer: [u8; 1024],
    buffer_index: usize,
}
impl<R: Read> RequestReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buffer: [0; 1024],
            buffer_index: 0,
        }
    }
    pub fn handle_request(&mut self) -> Result<Request, RequestError> {
        let mut request = Request::new();
        while !request.done() {
            if self.buffer_index == self.buffer.len() {
                return Err(RequestError::RequestTooLarge);
            }
            let bytes_read = self
                .reader
                .read(&mut self.buffer[self.buffer_index..])
                .map_err(RequestError::Io)?;
            if bytes_read == 0 {
                return Err(RequestError::UnexexpectedEndOfInput);
            }
            self.buffer_index += bytes_read;
            let read = request
                .parse(&self.buffer[..self.buffer_index])
                .map_err(RequestError::Parse)?;
            self.buffer.copy_within(read..self.buffer_index, 0);
            self.buffer_index -= read
        }
        Ok(request)
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
    #[test]
    fn should_parse_query_params() {
        let result = parse_request_line(b"GET /search?q=rust HTTP/1.1\r\n")
            .expect("Query params should be parsed successfully");
        let request_line = result.0;
        assert_eq!(request_line.request_target, "/search?q=rust")
    }
    #[test]
    fn should_fail_parse_unsupported_http_version() {
        let result = parse_request_line(b"GET / HTTP/2.0\r\n")
            .expect_err("Expected unsupported http version errror");
        assert_eq!(result, ParseError::UnsupportedHttpVersion)
    }
    #[test]
    fn should_fail_parse_malformed_http_version() {
        let result = parse_request_line(b"GET / HTTP/\r\n");
        assert!(result.is_err());
    }
    #[test]
    fn should_fail_parse_request_line_missing_parts() {
        let result = parse_request_line(b"GET / HTTP/1.1");
        assert!(result.is_err());
    }
    #[test]
    fn should_handle_empty_request_line() {
        let result = parse_request_line(b"");
        assert!(result.is_err());
    }
}
