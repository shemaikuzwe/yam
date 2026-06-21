use std::cmp::min;

use crate::headers::Headers;
#[derive(Debug)]
pub struct Request {
    pub request_line: RequestLine,
    pub headers: Headers,
    pub body: String,
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
const SEPARATOR: &str = "\r\n";
impl Request {
    pub fn parse(&mut self, data: &str) -> Result<usize, ParseError> {
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
                    self.request_line = result.0;
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
                    self.body.push_str(&curr_data[..remaining]);
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
fn parse_request_line(data: &str) -> Result<(RequestLine, usize), ParseError> {
    let idx = data
        .find(SEPARATOR)
        .ok_or(ParseError::IncompleteRequestLine)?;
    let request_line = &data[..idx];
    let read = idx + SEPARATOR.len();
    let mut parts = request_line.split(' ');

    let method = parts.next().ok_or(ParseError::InvalidRequestLine)?;
    let request_target = parts.next().ok_or(ParseError::InvalidRequestLine)?;
    let http = parts.next().ok_or(ParseError::InvalidRequestLine)?;

    // make sure there is not a 4th part
    if parts.next().is_some() {
        return Err(ParseError::InvalidRequestLine);
    }

    let mut http_parts = http.split('/');

    let protocol = http_parts.next().ok_or(ParseError::InvalidRequestLine)?;
    let version = http_parts.next().ok_or(ParseError::InvalidRequestLine)?;

    if protocol != "HTTP" || version != "1.1" || http_parts.next().is_some() {
        return Err(ParseError::UnsupportedHttpVersion);
    }

    Ok((
        RequestLine {
            method: method.to_string(),
            request_target: request_target.to_string(),
            http_version: version.to_string(),
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
            parse_request_line("GET / HTTP/1.1\r\n").expect("Valid request line should pass");

        let request_line = result.0;
        assert_eq!(request_line.http_version, "1.1");
        assert_eq!(request_line.method, "GET");
        assert_eq!(result.1, 16, "expected content length")
    }
    #[test]
    fn should_fail_parse_request_line() {
        let result = parse_request_line("GET / HTTP/1.1 foobar\r\n");
        assert!(result.is_err());
        println!("result {:#?}", result);
    }
}
