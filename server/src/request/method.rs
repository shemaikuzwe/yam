use std::fmt;

use super::error::Error;
use super::request::Request;

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum Method {
    GET,
    HEAD,
    POST,
    PATCH,
    PUT,
    DELETE,
    OPTIONS,
}

impl Method {
    /// Every supported method, in the order used for `Allow` headers.
    pub const ALL: [Method; 7] = [
        Method::GET,
        Method::HEAD,
        Method::POST,
        Method::PATCH,
        Method::PUT,
        Method::DELETE,
        Method::OPTIONS,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::GET => "GET",
            Self::HEAD => "HEAD",
            Self::POST => "POST",
            Self::PATCH => "PATCH",
            Self::PUT => "PUT",
            Self::DELETE => "DELETE",
            Self::OPTIONS => "OPTIONS",
        }
    }
}

impl TryFrom<&str> for Method {
    type Error = Error;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::ALL
            .into_iter()
            .find(|method| method.as_str().eq_ignore_ascii_case(value))
            .ok_or(Error::MethodNotAllowed)
    }
}

#[derive(PartialEq)]
pub enum ConnectionType {
    KeepAlive,
    Close,
    //Upgrade
}

impl fmt::Display for ConnectionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self {
            ConnectionType::Close => f.write_str("close"),
            ConnectionType::KeepAlive => f.write_str("keep-alive"),
        }
    }
}

pub fn get_connection_type(request: &Request) -> ConnectionType {
    let close = request
        .headers
        .get_all("connection")
        .flat_map(|value| value.split(','))
        .any(|token| token.trim().eq_ignore_ascii_case("close"));

    if close {
        ConnectionType::Close
    } else {
        ConnectionType::KeepAlive
    }
}
