use std::{
    cmp::min,
    collections::HashMap,
    io::{self},
    str::FromStr,
};

use serde::de::DeserializeOwned;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::response::HttpError;
use crate::{headers::Headers, request_ext::Extensions};
use yam_shared::HeaderParseError;

#[derive(Debug)]
pub struct Request {
    request_line: RequestLine,
    pub headers: Headers,
    pub body: Vec<u8>,
    path_params: HashMap<String, String>,
    parse_state: ParseState,
    extensions: Extensions,
}
#[derive(Debug, Default)]
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

impl From<HeaderParseError> for ParseError {
    fn from(error: HeaderParseError) -> Self {
        match error {
            HeaderParseError::IncompleteFieldLine => Self::IncompleteFieldLine,
            HeaderParseError::InvalidFieldLine => Self::InvalidFieldLine,
            HeaderParseError::InvalidHeaderKey => Self::InvalidHeaderKey,
        }
    }
}
const SEPARATOR: &[u8] = b"\r\n";

impl Default for Request {
    fn default() -> Self {
        Self::new()
    }
}
impl Request {
    pub fn new() -> Request {
        Request {
            headers: Headers::new(),
            body: Vec::new(),
            parse_state: ParseState::INIT,
            path_params: HashMap::new(),
            request_line: RequestLine::default(),
            extensions: Extensions::default(),
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
                    self.request_line = result.0;
                    read += result.1;
                    self.parse_state = ParseState::HEADERS
                }
                ParseState::HEADERS => {
                    let result = self.headers.parse(curr_data).map_err(ParseError::from)?;
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
    /// Deserializes the request body as JSON.
    ///
    /// ```
    /// use serde::Deserialize;
    /// use yam_server::{HttpError, Request};
    ///
    /// #[derive(Deserialize)]
    /// struct Login { email: String }
    ///
    /// let mut request = Request::new();
    /// request.body = br#"{"email":"user@example.com"}"#.to_vec();
    /// let login: Login = request.json()?;
    /// assert_eq!(login.email, "user@example.com");
    /// # Ok::<(), HttpError>(())
    /// ```
    pub fn json<T>(&self) -> Result<T, HttpError>
    where
        T: DeserializeOwned,
    {
        serde_json::from_slice(&self.body).map_err(HttpError::Json)
    }
    /// Reads the request body as UTF-8 text.
    /// ```
    /// use yam_server::{HttpError, Request};
    /// let mut request = Request::new();
    /// request.body = b"hello".to_vec();
    /// assert_eq!(request.text()?, "hello");
    /// # Ok::<(), HttpError>(())
    /// ```
    pub fn text(&self) -> Result<&str, HttpError> {
        std::str::from_utf8(&self.body).map_err(HttpError::Utf8)
    }
    /// Deserializes an `application/x-www-form-urlencoded` request body.
    /// Returns 400 BadRequest when an error occur.
    ///
    /// ```
    /// use serde::Deserialize;
    /// use yam_server::{HttpError, Request};
    /// #[derive(Deserialize)]
    /// struct Login { email: String }
    ///
    /// let mut request = Request::new();
    /// request.body = b"email=user%40example.com".to_vec();
    /// let login: Login = request.form_data()?;
    /// assert_eq!(login.email, "user@example.com");
    /// # Ok::<(), HttpError>(())
    /// ```
    pub fn form_data<T>(&self) -> Result<T, HttpError>
    where
        T: DeserializeOwned,
    {
        serde_urlencoded::from_bytes(&self.body).map_err(HttpError::Form)
    }
    /// Deserializes URL query parameters into a typed value.
    ///
    /// ```
    /// use serde::Deserialize;
    /// use yam_server::{HttpError, Request};
    /// #[derive(Deserialize)]
    /// struct Pagination { page: u64 }
    ///
    /// let mut request = Request::new();
    /// request.parse(b"GET /users?page=2 HTTP/1.1\r\n\r\n").unwrap();
    /// let pagination: Pagination = request.query()?;
    /// assert_eq!(pagination.page, 2);
    /// # Ok::<(), HttpError>(())
    /// ```
    pub fn query<T>(&self) -> Result<T, HttpError>
    where
        T: DeserializeOwned,
    {
        let query = &self.request_line.query.as_str();
        serde_urlencoded::from_str(query).map_err(HttpError::Form)
    }

    pub fn method(&self) -> &str {
        &self.request_line.method.as_str()
    }

    pub fn path(&self) -> &str {
        &self.request_line.request_target.as_str()
    }

    pub fn http_version(&self) -> &str {
        &self.request_line.http_version.as_str()
    }

    /// Returns a path parameter captured by a router pattern.
    ///
    /// ```no_run
    /// # use yam_server::Request;
    /// # fn handler(request: Request) {
    /// let id = request.param("id");
    /// # let _ = id;
    /// # }
    /// ```
    pub fn param(&self, name: &str) -> Option<&str> {
        self.path_params.get(name).map(String::as_str)
    }
    /// Parses a captured path parameter as `T`.
    ///
    /// ```no_run
    /// # use yam_server::{HttpError, Request};
    /// # fn handler(request: Request) -> Result<(), HttpError> {
    /// let id: u64 = request.param_as("id")?;
    /// # let _ = id;
    /// # Ok(())
    /// # }
    /// ```
    pub fn param_as<T>(&self, name: &str) -> Result<T, HttpError>
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
    /// Returns a cookie value from the request's `Cookie` headers.
    ///
    /// ```
    /// use yam_server::Request;
    /// let mut request = Request::new();
    /// request.headers.set("cookie", "session=abc123".into());
    /// assert_eq!(request.cookie("session"), Some("abc123"));
    /// ```
    pub fn cookie(&self, name: &str) -> Option<&str> {
        self.headers
            .get_all("cookie")
            .flat_map(|header| header.split(';'))
            .filter_map(|cookie| cookie.trim().split_once('='))
            .find_map(|(cookie_name, value)| (cookie_name.trim() == name).then_some(value.trim()))
    }
    /// Returns a reference to the request's [`Extensions`].
    ///
    /// ```
    /// use yam_server::Request;
    ///
    /// #[derive(Debug, PartialEq)]
    /// struct AuthUser { id: u64 }
    ///
    /// let mut request = Request::new();
    /// request.extensions_mut().insert(AuthUser { id: 1 });
    ///
    /// assert_eq!(request.extensions().get::<AuthUser>(), Some(&AuthUser { id: 1 }));
    /// ```
    pub fn extensions(&self) -> &Extensions {
        &self.extensions
    }
    /// Returns a mutable reference to the request's [`Extensions`].
    ///
    /// ```
    /// use yam_server::Request;
    ///
    /// #[derive(Debug, PartialEq)]
    /// struct AuthUser { id: u64 }
    ///
    /// let mut request = Request::new();
    /// request.extensions_mut().insert(AuthUser { id: 1 });
    /// ```
    pub fn extensions_mut(&mut self) -> &mut Extensions {
        &mut self.extensions
    }
}
#[derive(Debug, Error)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("parse error: {0}")]
    Parse(#[from] ParseError),
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

    #[derive(Debug, PartialEq)]
    struct AuthUser {
        id: u64,
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
            request_line: RequestLine::default(),
            headers: Headers::new(),
            body: br#"{"email":"user@example.com","password":"1234"}"#.to_vec(),
            parse_state: ParseState::DONE,
            extensions: Extensions::default(),
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
        request.request_line = RequestLine {
            http_version: "1.1".into(),
            request_target: "/list".into(),
            query: "page=2&per_page=30".into(),
            method: "GET".into(),
        };
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
        request.request_line = RequestLine {
            http_version: "1.1".into(),
            request_target: "/list".into(),
            query: "".into(),
            method: "GET".into(),
        };
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
            _page: u64,
        }
        let mut request = Request::new();
        request.request_line = RequestLine {
            http_version: "1.1".into(),
            request_target: "/list".into(),
            query: "page=hi".into(),
            method: "GET".into(),
        };
        assert!(request.query::<Pagination>().is_err());
    }
    #[test]
    fn query_should_percent_decode() {
        #[derive(Deserialize, Debug, PartialEq)]
        struct Search {
            q: String,
        }
        let mut request = Request::new();
        request.request_line = RequestLine {
            http_version: "1.1".into(),
            request_target: "/search".into(),
            query: "q=hello%20world".into(),
            method: "GET".into(),
        };
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
        Err(HttpError::Param(ParamError::Invalid(name))) if name == "id"
         ));
    }

    #[test]
    fn should_error_for_missing_typed_path_param() {
        let request = Request::new();

        assert!(matches!(
            request.param_as::<u64>("id"),
            Err(HttpError::Param(ParamError::Missing(name))) if name == "id"
        ));
    }

    #[test]
    fn should_store_and_read_extensions() {
        let mut request = Request::new();
        request.extensions_mut().insert(AuthUser { id: 1 });
        request.extensions_mut().insert(String::from("token"));

        assert_eq!(
            request.extensions().get::<AuthUser>(),
            Some(&AuthUser { id: 1 })
        );
        assert_eq!(
            request.extensions().get::<String>().map(String::as_str),
            Some("token")
        );
    }

    #[test]
    fn should_remove_extension_from_request() {
        let mut request = Request::new();
        request.extensions_mut().insert(AuthUser { id: 7 });

        assert_eq!(
            request.extensions_mut().remove::<AuthUser>(),
            Some(AuthUser { id: 7 })
        );
        assert_eq!(request.extensions().get::<AuthUser>(), None);
    }
}
