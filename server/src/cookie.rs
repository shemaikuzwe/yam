use std::{fmt, time::Duration, write};

#[derive(Debug)]
pub struct Cookie {
    name: String,
    value: String,
    path: Option<String>,
    http_only: bool,
    secure: bool,
    domain: Option<String>,
    same_site: SameSite,
    max_age: Option<Duration>,
}
#[derive(Debug)]
pub enum SameSite {
    Strict,
    Lax,
    None,
}

impl fmt::Display for SameSite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SameSite::Lax => f.write_str("Lax")?,
            SameSite::Strict => f.write_str("Strict")?,
            Self::None => f.write_str("None")?,
        };
        Ok(())
    }
}

impl Cookie {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            path: None,
            http_only: false,
            secure: false,
            domain: None,
            same_site: SameSite::Lax,
            max_age: None,
        }
    }

    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub fn http_only(mut self, http_only: bool) -> Self {
        self.http_only = http_only;
        self
    }

    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }
    pub fn same_site(mut self, same_site: SameSite) -> Self {
        self.same_site = same_site;
        self
    }
    pub fn domain(mut self, domain: &str) -> Self {
        self.domain = Some(domain.to_string());
        self
    }
    pub fn max_age(mut self, max_age: Duration) -> Self {
        self.max_age = Some(max_age);
        self
    }
}

impl fmt::Display for Cookie {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}={}", self.name, self.value)?;
        write!(formatter, "; SameSite={}", self.same_site.to_string())?;
        if let Some(path) = &self.path {
            write!(formatter, "; Path={path}")?;
        }
        if self.http_only {
            write!(formatter, "; HttpOnly")?;
        }
        if self.secure {
            write!(formatter, "; Secure")?;
        }
        if let Some(domain) = &self.domain {
            write!(formatter, "; Domain={domain}")?;
        }
        if let Some(max_age) = &self.max_age {
            write!(formatter, "; Max-Age={}", max_age.as_secs())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_cookie_attributes() {
        let cookie = Cookie::new("session", "abc123")
            .path("/")
            .http_only(true)
            .secure(true);

        assert_eq!(
            "session=abc123; SameSite=Lax; Path=/; HttpOnly; Secure",
            cookie.to_string()
        );
    }
}
