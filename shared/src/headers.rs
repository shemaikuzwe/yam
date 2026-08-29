use std::{collections::HashMap, str};

use thiserror::Error;

const SEPARATOR: &[u8] = b"\r\n";

#[derive(Debug, Default, Clone)]
pub struct Headers {
    headers: HashMap<String, Vec<String>>,
}

impl Headers {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the first value associated with `name`.
    ///
    /// ```
    /// use yam_shared::Headers;
    ///
    /// let mut headers = Headers::new();
    /// headers.set("content-type", "application/json".into());
    ///
    /// assert_eq!(headers.get("Content-Type"), Some("application/json"));
    /// ```
    pub fn get(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_lowercase())?
            .first()
            .map(String::as_str)
    }

    /// Replaces all values associated with `name`.
    pub fn set(&mut self, name: &str, value: String) {
        self.headers.insert(name.to_lowercase(), vec![value]);
    }

    /// Adds a value without replacing values already associated with `name`.
    pub fn append(&mut self, name: &str, value: String) {
        self.headers
            .entry(name.to_lowercase())
            .or_default()
            .push(value);
    }

    /// Returns all values associated with `name`.
    ///
    /// ```
    /// use yam_shared::Headers;
    ///
    /// let mut headers = Headers::new();
    /// headers.append("accept", "text/html".into());
    /// headers.append("accept", "application/json".into());
    ///
    /// let values = headers.get_all("Accept").collect::<Vec<_>>();
    /// assert_eq!(values, ["text/html", "application/json"]);
    /// ```
    pub fn get_all(&self, name: &str) -> impl Iterator<Item = &str> {
        self.headers
            .get(&name.to_lowercase())
            .into_iter()
            .flatten()
            .map(String::as_str)
    }

    /// Iterates over every header name and value pair.
    ///
    /// Names with multiple values are yielded once for each value.
    ///
    /// ```
    /// use yam_shared::Headers;
    ///
    /// let mut headers = Headers::new();
    /// headers.set("content-length", "12".into());
    ///
    /// assert_eq!(headers.iter().collect::<Vec<_>>(), [("content-length", "12")]);
    /// ```
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.headers.iter().flat_map(|(name, values)| {
            values
                .iter()
                .map(move |value| (name.as_str(), value.as_str()))
        })
    }
    pub fn parse(&mut self, data: &[u8]) -> Result<(usize, bool), HeaderParseError> {
        let mut read = 0;
        let mut done = false;
        loop {
            let current_data = &data[read..];
            let Some(idx) = current_data
                .windows(SEPARATOR.len())
                .position(|window| window == SEPARATOR)
            else {
                break;
            };
            if idx == 0 {
                read += SEPARATOR.len();
                done = true;
                break;
            }
            let (name, value) = parse_header(&current_data[..idx])?;
            self.append(name, value.to_string());
            read += idx + SEPARATOR.len();
        }
        Ok((read, done))
    }
}

#[derive(Debug, PartialEq, Error)]
pub enum HeaderParseError {
    #[error("incomplete field line")]
    IncompleteFieldLine,
    #[error("invalid field line")]
    InvalidFieldLine,
    #[error("invalid header key")]
    InvalidHeaderKey,
}

fn parse_header(field_line: &[u8]) -> Result<(&str, &str), HeaderParseError> {
    let mut parts = field_line.splitn(2, |&byte| byte == b':');
    let key = parts.next().ok_or(HeaderParseError::InvalidFieldLine)?;
    let key = str::from_utf8(key).map_err(|_| HeaderParseError::IncompleteFieldLine)?;
    let value = parts.next().ok_or(HeaderParseError::IncompleteFieldLine)?;
    let value = str::from_utf8(value)
        .map_err(|_| HeaderParseError::IncompleteFieldLine)?
        .trim();
    if key.ends_with(' ') || !is_valid_token(key) {
        return Err(HeaderParseError::InvalidHeaderKey);
    }
    Ok((key, value))
}

fn is_valid_token(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_repeated_headers() {
        let mut headers = Headers::new();
        headers
            .parse(b"accept: text/html\r\naccept: application/json\r\n\r\n")
            .expect("headers should parse");

        assert_eq!(
            vec!["text/html", "application/json"],
            headers.get_all("accept").collect::<Vec<_>>()
        );
    }

    #[test]
    fn set_replaces_existing_values() {
        let mut headers = Headers::new();
        headers.append("accept", "text/html".into());
        headers.append("accept", "application/json".into());
        headers.set("accept", "text/plain".into());

        assert_eq!(
            vec!["text/plain"],
            headers.get_all("accept").collect::<Vec<_>>()
        );
    }
}
