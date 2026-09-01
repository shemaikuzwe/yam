use std::{any::{Any, TypeId}, collections::HashMap};

#[derive(Debug, Default)]
pub struct Extensions{
    map: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}
impl Extensions {
    /// Stores `val` under its type, replacing any value previously stored for `T`.
    ///
    /// ```
    /// use yam_server::Extensions;
    ///
    /// #[derive(Debug, PartialEq)]
    /// struct AuthUser { id: u64 }
    ///
    /// let mut extensions = Extensions::default();
    /// extensions.insert(AuthUser { id: 1 });
    /// extensions.insert(AuthUser { id: 2 });
    /// assert_eq!(extensions.get::<AuthUser>(), Some(&AuthUser { id: 2 }));
    /// ```
    pub fn insert<T:Any+Send+Sync>(&mut self,val:T){
          self.map.insert(TypeId::of::<T>(), Box::new(val));
    }
    /// Returns a reference to the value stored for type `T`, or `None` if
    /// nothing was stored for it.
    ///
    /// ```
    /// use yam_server::Extensions;
    ///
    /// #[derive(Debug, PartialEq)]
    /// struct AuthUser { id: u64 }
    ///
    /// let mut extensions = Extensions::default();
    /// extensions.insert(AuthUser { id: 1 });
    ///
    /// assert_eq!(extensions.get::<AuthUser>(), Some(&AuthUser { id: 1 }));
    /// assert_eq!(extensions.get::<String>(), None);
    /// ```
    pub fn get<T: Any + Send + Sync>(&self) -> Option<&T> {
           self.map.get(&TypeId::of::<T>())?.downcast_ref::<T>()
    }
    /// Returns a mutable reference to the value stored for type `T`, or
    /// `None` if nothing was stored for it.
    ///
    /// ```
    /// use yam_server::Extensions;
    ///
    /// struct RateLimit { remaining: u32 }
    ///
    /// let mut extensions = Extensions::default();
    /// extensions.insert(RateLimit { remaining: 5 });
    ///
    /// if let Some(limit) = extensions.get_mut::<RateLimit>() {
    ///     limit.remaining -= 1;
    /// }
    /// assert_eq!(extensions.get::<RateLimit>().map(|l| l.remaining), Some(4));
    /// ```
    pub fn get_mut<T: Any + Send + Sync>(&mut self) -> Option<&mut T> {
           self.map.get_mut(&TypeId::of::<T>())?.downcast_mut::<T>()
    }
    /// Removes and returns the value stored for type `T`, or `None` if
    /// nothing was stored for it.
    ///
    /// ```
    /// use yam_server::Extensions;
    ///
    /// let mut extensions = Extensions::default();
    /// extensions.insert("jwt-token".to_string());
    ///
    /// let token = extensions.remove::<String>();
    /// assert_eq!(token.as_deref(), Some("jwt-token"));
    /// assert_eq!(extensions.get::<String>(), None);
    /// ```
    pub fn remove<T: Any + Send + Sync>(&mut self) -> Option<T> {
           self.map.remove(&TypeId::of::<T>())?.downcast::<T>().ok().map(|b| *b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_get_inserted_value() {
        let mut extensions = Extensions::default();
        extensions.insert(String::from("token"));

        assert_eq!(
            extensions.get::<String>().map(String::as_str),
            Some("token")
        );
    }

    #[test]
    fn should_return_none_for_missing_type() {
        let extensions = Extensions::default();

        assert_eq!(extensions.get::<String>(), None);
    }

    #[test]
    fn should_replace_value_of_same_type() {
        let mut extensions = Extensions::default();
        extensions.insert(1u64);
        extensions.insert(2u64);

        assert_eq!(extensions.get::<u64>().copied(), Some(2));
    }

    #[test]
    fn should_store_different_types_side_by_side() {
        let mut extensions = Extensions::default();
        extensions.insert(String::from("user"));
        extensions.insert(42u64);

        assert_eq!(
            extensions.get::<String>().map(String::as_str),
            Some("user")
        );
        assert_eq!(extensions.get::<u64>().copied(), Some(42));
    }

    #[test]
    fn should_mutate_value_through_get_mut() {
        let mut extensions = Extensions::default();
        extensions.insert(vec![1, 2]);

        if let Some(values) = extensions.get_mut::<Vec<i32>>() {
            values.push(3);
        }

        assert_eq!(extensions.get::<Vec<i32>>(), Some(&vec![1, 2, 3]));
    }

    #[test]
    fn should_remove_stored_value() {
        let mut extensions = Extensions::default();
        extensions.insert(String::from("token"));

        assert_eq!(extensions.remove::<String>(), Some(String::from("token")));
        assert_eq!(extensions.get::<String>(), None);
        assert_eq!(extensions.remove::<String>(), None);
    }

    #[test]
    fn should_not_remove_values_of_other_types() {
        let mut extensions = Extensions::default();
        extensions.insert(String::from("user"));

        assert_eq!(extensions.remove::<u64>(), None);
        assert_eq!(
            extensions.get::<String>().map(String::as_str),
            Some("user")
        );
    }
}
