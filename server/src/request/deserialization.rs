use serde::de::DeserializeOwned;

use crate::response::HttpError;

use super::request::Request;

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
}

#[cfg(test)]
mod tests {

    use serde::Deserialize;


    use super::*;

    use super::super::request_line::RequestLine;

    #[derive(Deserialize)]
    struct Login {
        email: String,
        password: String,
    }

    #[test]
    fn should_desirialize_valid_json() {
        let mut request = Request::new();
        request.body = br#"{"email":"user@example.com","password":"1234"}"#.to_vec();
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
    fn should_return_utf8_string() {
        let mut request = Request::new();
        request.body = b"hello".to_vec();
        assert_eq!(request.text().expect("utf8"), "hello");
    }

    #[test]
    fn should_deserialize_urlencoded() {
        let mut request = Request::new();
        request.body = b"email=user%40example.com&password=1234".to_vec();
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
}
