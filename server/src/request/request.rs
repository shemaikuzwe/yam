use std::{cmp::min, collections::HashMap};

use crate::headers::Headers;

use super::error::ParseError;
use super::request_ext::Extensions;
use super::request_line::{parse_request_line, RequestLine};

#[derive(Debug)]
pub struct Request {
    pub(super) request_line: RequestLine,
    pub headers: Headers,
    pub body: Vec<u8>,
    pub(super) path_params: HashMap<String, String>,
    pub(super) parse_state: ParseState,
    pub(super) extensions: Extensions,
}

#[derive(Debug, PartialEq)]
pub enum ParseState {
    Init,
    Headers,
    Body,
    Done,
}

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
            parse_state: ParseState::Init,
            path_params: HashMap::new(),
            request_line: RequestLine::default(),
            extensions: Extensions::default(),
        }
    }

    pub(super) fn done(&self) -> bool {
        self.parse_state == ParseState::Done
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
                ParseState::Init => {
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
                    self.parse_state = ParseState::Headers
                }
                ParseState::Headers => {
                    let result = self.headers.parse(curr_data).map_err(ParseError::from)?;
                    if result.0 == 0 {
                        return Ok(read);
                    }
                    read += result.0;
                    if result.1 {
                        if self.has_body() {
                            self.parse_state = ParseState::Body
                        } else {
                            self.parse_state = ParseState::Done
                        }
                    }
                }
                ParseState::Body => {
                    const MAX_BODY_SIZE: usize = 2 * 1024 * 1024;
                    let length = self
                        .headers
                        .get("content-length")
                        .ok_or(ParseError::InvalidContentLengthHeader)?
                        .parse::<usize>()
                        .map_err(|_| ParseError::InvalidContentLengthHeader)?;
                    if length == 0 {
                        self.parse_state = ParseState::Done;
                        return Ok(read);
                    }
                    if length > MAX_BODY_SIZE {
                        return Err(ParseError::RequestTooLarge);
                    }
                    let remaining = min(curr_data.len(), length - self.body.len());
                    self.body.extend_from_slice(&curr_data[..remaining]);
                    read += remaining;
                    if self.body.len() == length {
                        self.parse_state = ParseState::Done;
                        return Ok(read);
                    }
                    if remaining == 0 && self.body.len() != length {
                        return Err(ParseError::InvalidContentLengthHeader);
                    }
                }
                ParseState::Done => {
                    return Ok(read);
                }
            }
        }
    }

    pub fn method(&self) -> &str {
        self.request_line.method.as_str()
    }

    pub fn path(&self) -> &str {
        self.request_line.request_target.as_str()
    }

    pub fn http_version(&self) -> &str {
        self.request_line.http_version.as_str()
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct AuthUser {
        id: u64,
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
