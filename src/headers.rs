use crate::request::ParseError;
use std::collections::HashMap;
const SEPARATOR: &[u8] = b"\r\n";

#[derive(Debug)]
pub struct Headers {
    pub headers: HashMap<String, String>,
}
impl Headers {
    pub fn new() -> Headers {
        Headers {
            headers: HashMap::new(),
        }
    }
    pub fn get(&self, name: &str) -> Option<&String> {
        let name = &name.to_lowercase();
        self.headers.get(name)
    }
    pub fn set(&mut self, name: &str, value: String) {
        let name = name.to_lowercase();
        self.headers.insert(name, value);
    }
    pub fn append(&mut self, name: &str, value: String) {
        let name = name.to_lowercase();
        if can_append_header(name.as_str()) {
            if let Some(val) = self.headers.get(name.as_str()) {
                self.headers.insert(name, format!("{},{}", val, value));
            } else {
                self.headers.insert(name, value);
            }
        } else {
            self.headers.insert(name, value);
        }
    }
    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.headers.iter()
    }
    pub fn parse(&mut self, data: &[u8]) -> Result<(usize, bool), ParseError> {
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
            //empy line parsing completed
            if idx == 0 {
                read += SEPARATOR.len();
                done = true;
                break;
            }
            let header_line = &current_data[..idx];
            let header_line = parse_header(header_line)?;
            if !is_valid_token(header_line.0) {
                return Err(ParseError::InvalidHeaderKey);
            }
            self.append(header_line.0, header_line.1.to_string());
            read += idx + SEPARATOR.len();
        }
        Ok((read, done))
    }
}
fn parse_header(field_line: &[u8]) -> Result<(&str, &str), ParseError> {
    let mut parts = field_line.splitn(2, |&x| x == b':');

    let key = parts.next().ok_or(ParseError::InvalidFieldLine)?;

    let key = str::from_utf8(key).map_err(|_| ParseError::IncompleteFieldLine)?;
    let val = parts.next().ok_or(ParseError::IncompleteFieldLine)?;
    let val = str::from_utf8(val)
        .map_err(|_| ParseError::IncompleteFieldLine)?
        .trim();
    if key.ends_with(" ") {
        return Err(ParseError::InvalidFieldLine);
    }
    Ok((key, val))
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

fn can_append_header(name: &str) -> bool {
    matches!(
        name,
        "accept" | "accept-encoding" | "accept-language" | "cache-control" | "warning" | "via"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn valid_single_header() {
        let mut headers = Headers::new();
        let data = b"HOst: localhost:42069\r\nFOoFoo: barbar\r\n\r\n";
        let result = headers
            .parse(data)
            .expect("it should have passed single header");
        assert_eq!(41, result.0);
        let host = headers
            .get("host")
            .expect("should not get error while header.get(host)");

        let foo = headers
            .get("foofoo")
            .expect("should not get error while header.get(host)");
        assert_eq!("localhost:42069", host);
        assert_eq!("barbar", foo);
    }
    #[test]
    fn bad_header_key() {
        let mut headers = Headers::new();
        let data = b"H[st: localhost:42069\r\n\r\n";
        let result = headers.parse(data).expect_err("We should get error here.");
        assert_eq!(result, ParseError::InvalidHeaderKey);
    }
    #[test]
    fn incomplete_header() {
        let mut headers = Headers::new();
        let data = b"Host: localhost\r\nUser-Agent: curl\r\n";
        let result = headers.parse(data).expect("Should not return an error");
        assert!(!result.1, "not done")
    }
    #[test]
    fn multiple_headers() {
        let mut headers = Headers::new();
        let data = b"accept: text/html\r\naccept: application/json\r\n\r\n";
        headers.parse(data).expect("Should noot get error here");
        let accept = headers
            .get("accept")
            .expect("should not get error while header.get(accept)");
        assert_eq!("text/html,application/json", accept);
    }
}
