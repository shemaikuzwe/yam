use std::collections::HashMap;

#[derive(Debug, PartialEq)]
pub enum ParseError {
    IncompleteFieldLine,
    InvalidFieldLine,
    InvalidHeaderKey,
}
const SEPARATOR: &str = "\r\n";

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
        if let Some(val) = self.headers.get(name.as_str()) {
            self.headers.insert(name, format!("{},{}", val, value));
        } else {
            self.headers.insert(name, value);
        }
    }
    pub fn replace(&mut self, name: String, value: String) {
        let name = name.to_lowercase();
        self.headers.insert(name, value);
    }
    pub fn delete(&mut self, name: &str) {
        let name = name.to_lowercase();
        self.headers.remove(name.as_str());
    }
    pub fn parse(&mut self, data: &str) -> Result<(usize, bool), ParseError> {
        let mut read = 0;
        loop {
            let current_data = &data[read..];
            let Some(idx) = current_data.find(SEPARATOR) else {
                return Ok((read, false));
            };
            //empy line parsing completed
            if idx == 0 {
                read += SEPARATOR.len();
                return Ok((read, true));
            }
            let header_line = &current_data[..idx];
            let header_line = parse_header(header_line)?;
            if !is_valid_token(header_line.0) {
                return Err(ParseError::InvalidHeaderKey);
            }
            self.set(header_line.0, header_line.1.to_string());
            read += idx + SEPARATOR.len();
        }
    }
}
fn parse_header(field_line: &str) -> Result<(&str, &str), ParseError> {
    let parts = field_line
        .split_once(":")
        .ok_or(ParseError::InvalidFieldLine)?;
    let key = parts.0;
    let val = parts.1.trim();
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn valid_single_header() {
        let mut headers = Headers::new();
        let data = "HOst: localhost:42069\r\nFOoFoo: barbar\r\n\r\n";
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
        let data = "H©st: localhost:42069\r\n\r\n";
        let result = headers.parse(data).expect_err("We should get error here.");
        assert_eq!(result, ParseError::InvalidHeaderKey);
    }
    #[test]
    fn multiple_headers() {
        let mut headers = Headers::new();
        let data = "Host: localhost:42069\r\nHOst: localhost:3000\r\n\r\n";
        headers.parse(data).expect("Should noot get error here");
        let host = headers
            .get("host")
            .expect("should not get error while header.get(host)");
        assert_eq!("localhost:42069,localhost:3000", host);
        assert_ne!(" localhost:42069,localhost:3000", host);
    }
}
