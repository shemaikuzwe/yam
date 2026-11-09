use super::error::ParseError;

const SEPARATOR: &[u8] = b"\r\n";

#[derive(Debug, Default)]
pub struct RequestLine {
    pub http_version: String,
    pub request_target: String,
    pub query: String,
    pub method: String,
}

pub fn parse_request_line(data: &[u8]) -> Result<(RequestLine, usize), ParseError> {
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

    // makes sure there is not a 4th part
    if parts.next().is_some() {
        return Err(ParseError::InvalidRequestLine);
    }

    let mut http_parts = http.split(|&x| x == b'/');

    let protocol = http_parts.next().ok_or(ParseError::InvalidRequestLine)?;
    let version = http_parts.next().ok_or(ParseError::InvalidRequestLine)?;

    if protocol != b"HTTP" || version != b"1.1" || http_parts.next().is_some() {
        return Err(ParseError::UnsupportedHttpVersion);
    }
    let request_target =
        std::str::from_utf8(request_target).map_err(|_| ParseError::InvalidRequestLine)?;
    let (request_target, query) = match request_target.split_once('?') {
        Some((path, query)) => (path, query),
        None => (request_target, ""),
    };

    Ok((
        RequestLine {
            method: std::str::from_utf8(method)
                .map_err(|_| ParseError::InvalidRequestLine)?
                .to_string(),
            request_target: request_target.to_string(),
            query: query.to_string(),
            http_version: std::str::from_utf8(version)
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
        assert_eq!(request_line.request_target, "/search");
        assert_eq!(request_line.query, "q=rust");
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
