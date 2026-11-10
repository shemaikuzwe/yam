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

impl TryFrom<&str> for Method {
    type Error = Error;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.to_uppercase().as_str() {
            "GET" => Ok(Self::GET),
            "HEAD" => Ok(Self::HEAD),
            "POST" => Ok(Self::POST),
            "PATCH" => Ok(Self::PATCH),
            "PUT" => Ok(Self::PUT),
            "DELETE" => Ok(Self::DELETE),
            "OPTIONS" => Ok(Self::OPTIONS),
            _ => Err(Error::MethodNotAllowed),
        }
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
