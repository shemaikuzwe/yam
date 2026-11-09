use std::str::FromStr;

use crate::response::HttpError;

use super::error::ParamError;
use super::request::Request;

impl Request {
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

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, matches};

    use crate::{HttpError, Request};

    use super::ParamError;

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
}
