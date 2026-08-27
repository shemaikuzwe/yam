use std::{
    cmp::min,
    collections::HashMap,
    io::{self},
    str::FromStr,
};

use serde::de::DeserializeOwned;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::headers::Headers;

#[derive(Debug)]
pub struct Request {
    request_line: Option<RequestLine>,
    pub headers: Headers,
    pub body: Vec<u8>,
    path_params: HashMap<String, String>,
    parse_state: ParseState,
}
#[derive(Debug)]
pub struct RequestLine {
    pub http_version: String,
    pub request_target: String,
    pub query: String,
    pub method: String,
}
#[derive(Debug, PartialEq)]
enum ParseState {
    INIT,
    HEADERS,
    BODY,
    DONE,
}
#[derive(Debug, PartialEq, Error)]
pub enum ParseError {
    #[error("incomplete request line")]
    IncompleteRequestLine,
    #[error("invalid request line")]
    InvalidRequestLine,
    #[error("unsupported http version")]
    UnsupportedHttpVersion,
    #[error("incomplete field line")]
    IncompleteFieldLine,
    #[error("invalid field line")]
    InvalidFieldLine,
    #[error("invalid header key")]
    InvalidHeaderKey,
    #[error("invalid content-length header")]
    InvalidContentLengthHeader,
}
const SEPARATOR: &[u8] = b"\r\n";
impl Request {
    pub fn new() -> Request {
        Request {
            headers: Headers::new(),
            body: Vec::new(),
            parse_state: ParseState::INIT,
            path_params: HashMap::new(),
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

//deserialization
impl Request {
    pub fn json<T>(&self) -> Result<T, serde_json::Error>
    where
        T: DeserializeOwned,
    {
        serde_json::from_slice(&self.body)
    }
    pub fn text(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.body)
    }
    pub fn form_data<T>(&self) -> Result<T, serde_urlencoded::de::Error>
    where
        T: DeserializeOwned,
    {
        serde_urlencoded::from_bytes(&self.body)
    }
    pub fn query<T>(&self) -> Result<T, serde_urlencoded::de::Error>
    where
        T: DeserializeOwned,
    {
        let query = self
            .request_line
            .as_ref()
            .map(|line| line.query.as_str())
            .unwrap_or("");
        serde_urlencoded::from_str(query)
    }

    pub fn method(&self) -> Option<&str> {
        self.request_line.as_ref().map(|line| line.method.as_str())
    }

    pub fn path(&self) -> Option<&str> {
        self.request_line
            .as_ref()
            .map(|line| line.request_target.as_str())
    }

    pub fn http_version(&self) -> Option<&str> {
        self.request_line
            .as_ref()
            .map(|line| line.http_version.as_str())
    }

    pub fn param(&self, name: &str) -> Option<&str> {
        self.path_params.get(name).map(String::as_str)
    }
    pub fn param_as<T>(&self, name: &str) -> Result<T, Error>
    where
        T: FromStr,
    {
        let value = self
            .param(name)
            .ok_or_else(|| ParamError::Missing(name.to_string()))?;
        value
            .parse::<T>()
            .map_err(|_| ParamError::Invalid(name.to_string()).into())
    }
}

impl Request {
    pub fn set_params(&mut self, params: HashMap<String, String>) {
        self.path_params = params
    }
    pub fn cookie(&self, name: &str) -> Option<&str> {
        self.headers
            .get_all("cookie")
            .flat_map(|header| header.split(';'))
            .filter_map(|cookie| cookie.trim().split_once('='))
            .find_map(|(cookie_name, value)| (cookie_name.trim() == name).then_some(value.trim()))
    }
}
#[derive(Debug, Error)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("parse error: {0}")]
    Parse(#[from] ParseError),
    #[error("{0}")]
    Param(#[from] ParamError),
    #[error("request too large")]
    RequestTooLarge,
    #[error("unexpected end of input")]
    UnexexpectedEndOfInput,
    #[error("method not allowed")]
    MethodNotAllowed,
}
#[derive(Debug, Error, PartialEq)]
pub enum ParamError {
    #[error("missing param: {0}")]
    Missing(String),
    #[error("invalid typed param: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum Method {
    GET,
    POST,
    PATCH,
    PUT,
    DELETE,
}

pub struct RequestReader<R> {
    reader: R,
    buffer: [u8; 1024],
    buffer_index: usize,
}

impl TryFrom<&str> for Method {
    type Error = Error;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.to_uppercase().as_str() {
            "GET" => Ok(Self::GET),
            "POST" => Ok(Self::POST),
            "PATCH" => Ok(Self::PATCH),
            "PUT" => Ok(Self::PUT),
            "DELETE" => Ok(Self::DELETE),
            _ => Err(Error::MethodNotAllowed),
        }
    }
}

impl<R: AsyncRead + Unpin> RequestReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buffer: [0; 1024],
            buffer_index: 0,
        }
    }
    pub async fn handle_request(&mut self) -> Result<Request, Error> {
        let mut request = Request::new();
        while !request.done() {
            if self.buffer_index == self.buffer.len() {
                return Err(Error::RequestTooLarge);
            }
            let bytes_read = self
                .reader
                .read(&mut self.buffer[self.buffer_index..])
                .await
                .map_err(Error::Io)?;
            if bytes_read == 0 {
                return Err(Error::UnexexpectedEndOfInput);
            }
            self.buffer_index += bytes_read;
            let read = request
                .parse(&self.buffer[..self.buffer_index])
                .map_err(Error::Parse)?;
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
    let request_target =
        str::from_utf8(request_target).map_err(|_| ParseError::InvalidRequestLine)?;
    let (request_target, query) = match request_target.split_once('?') {
        Some((path, query)) => (path, query),
        None => (request_target, ""),
    };

    Ok((
        RequestLine {
            method: str::from_utf8(method)
                .map_err(|_| ParseError::InvalidRequestLine)?
                .to_string(),
            request_target: request_target.to_string(),
            query: query.to_string(),
            http_version: str::from_utf8(version)
                .map_err(|_| ParseError::InvalidRequestLine)?
                .to_string(),
        },
        read,
    ))
}

#[cfg(test)]
mod tests {
    use std::{assert_eq, matches};

    use serde::Deserialize;

    use super::*;

    #[derive(Deserialize)]
    struct Login {
        email: String,
        password: String,
    }
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
    #[test]
    fn should_desirialize_valid_json() {
        let request = Request {
            path_params: HashMap::new(),
            request_line: None,
            headers: Headers::new(),
            body: br#"{"email":"user@example.com","password":"1234"}"#.to_vec(),
            parse_state: ParseState::DONE,
        };
        let login: Login = request.json().expect("Should be desirialized");
        assert_eq!(login.email, "user@example.com");
        assert_eq!(login.password, "1234");
    }
    #[test]
    fn json_should_error_on_invalid_json() {
        let request = Request {
            body: b"{not json".to_vec(),
            ..Request::new()
        };
        assert!(request.json::<Login>().is_err());
    }
    #[test]
    fn should_get_cookie_from_repeated_cookie_headers() {
        let mut request = Request::new();
        request
            .headers
            .append("cookie", "session=abc=123; theme=dark".to_string());
        request.headers.append("cookie", "locale=en".to_string());

        assert_eq!(Some("abc=123"), request.cookie("session"));
        assert_eq!(Some("dark"), request.cookie("theme"));
        assert_eq!(Some("en"), request.cookie("locale"));
        assert_eq!(None, request.cookie("missing"));
    }
    #[test]
    fn should_return_utf8_string() {
        let request = Request {
            body: b"hello".to_vec(),
            ..Request::new()
        };
        assert_eq!(request.text().expect("utf8"), "hello");
    }
    #[test]
    fn should_deserialize_urlencoded() {
        let request = Request {
            body: b"email=user%40example.com&password=1234".to_vec(),
            ..Request::new()
        };
        let form: Login = request.form_data().expect("valid form should deserialize");
        assert_eq!(form.email, "user@example.com");
        assert_eq!(form.password, "1234");
    }
    #[test]
    fn should_deserialize_query_params() {
        #[derive(Deserialize, Debug, PartialEq)]
        struct Pagination {
            page: Option<u64>,
            per_page: Option<u64>,
        }
        let mut request = Request::new();
        request.request_line = Some(RequestLine {
            http_version: "1.1".into(),
            request_target: "/list".into(),
            query: "page=2&per_page=30".into(),
            method: "GET".into(),
        });
        let pagination: Pagination = request.query().expect("valid query should deserialize");
        assert_eq!(
            pagination,
            Pagination {
                page: Some(2),
                per_page: Some(30),
            }
        );
    }
    #[test]
    fn should_deserialize_missing_query_to_defaults() {
        #[derive(Deserialize, Debug, PartialEq)]
        struct Pagination {
            page: Option<u64>,
            per_page: Option<u64>,
        }
        let mut request = Request::new();
        request.request_line = Some(RequestLine {
            http_version: "1.1".into(),
            request_target: "/list".into(),
            query: "".into(),
            method: "GET".into(),
        });
        let pagination: Pagination = request.query().expect("empty query should deserialize");
        assert_eq!(
            pagination,
            Pagination {
                page: None,
                per_page: None,
            }
        );
    }
    #[test]
    fn query_should_error_on_invalid_query() {
        #[derive(Deserialize)]
        struct Pagination {
            page: u64,
        }
        let mut request = Request::new();
        request.request_line = Some(RequestLine {
            http_version: "1.1".into(),
            request_target: "/list".into(),
            query: "page=hi".into(),
            method: "GET".into(),
        });
        assert!(request.query::<Pagination>().is_err());
    }
    #[test]
    fn query_should_percent_decode() {
        #[derive(Deserialize, Debug, PartialEq)]
        struct Search {
            q: String,
        }
        let mut request = Request::new();
        request.request_line = Some(RequestLine {
            http_version: "1.1".into(),
            request_target: "/search".into(),
            query: "q=hello%20world".into(),
            method: "GET".into(),
        });
        let search: Search = request
            .query()
            .expect("percent encoded query should deserialize");
        assert_eq!(search.q, "hello world");
    }
    #[test]
    fn should_return_param() {
        let mut request = Request::new();
        request.set_params(HashMap::from([("year".to_string(), "2026".to_string())]));
        assert_eq!(request.param("year"), Some("2026"));
        assert_eq!(request.param("none"), None);
    }
    #[test]
    fn should_parse_type_param() {
        let mut request = Request::new();
        request.set_params(HashMap::from([("year".to_string(), "2026".to_string())]));
        assert!(matches!(request.param_as::<u64>("year"), Ok(2026)));
    }
    #[test]
    fn should_error_for_invalid_typed_path_param() {
        let mut request = Request::new();

        request.set_params(HashMap::from([(
            "id".to_string(),
            "not-a-number".to_string(),
        )]));

        assert!(matches!(
        request.param_as::<u64>("id"),
        Err(Error::Param(ParamError::Invalid(name))) if name == "id"
         ));
    }

    #[test]
    fn should_error_for_missing_typed_path_param() {
        let request = Request::new();

        assert!(matches!(
            request.param_as::<u64>("id"),
            Err(Error::Param(ParamError::Missing(name))) if name == "id"
        ));
    }
}
