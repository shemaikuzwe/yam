use std::{collections::HashMap, sync::Arc};

use matchit::Router as MatchRouter;
use percent_encoding::percent_decode_str;
use yam_server::{Handler, request::Method};

pub(crate) enum Lookup {
    Found {
        handler: Arc<dyn Handler>,
        params: HashMap<String, String>,
    },
    WrongMethod {
        allow: String,
    },
    NotFound,
}

pub(crate) struct Routes {
    table: HashMap<Method, MatchRouter<Arc<dyn Handler>>>,
    strict_trailing_slash: bool,
}

impl Routes {
    pub(crate) fn new(strict_trailing_slash: bool) -> Self {
        Self {
            table: HashMap::new(),
            strict_trailing_slash,
        }
    }

    pub(crate) fn insert(&mut self, method: Method, path: &str, handler: Arc<dyn Handler>) {
        let path = self.normalize_path(path);
        // A GET route also answers HEAD; the server drops the body.
        let methods = if method == Method::GET {
            vec![Method::GET, Method::HEAD]
        } else {
            vec![method]
        };
        for method in methods {
            self.table
                .entry(method)
                .or_default()
                .insert(path, Arc::clone(&handler))
                .unwrap_or_else(|err| panic!("Failed to register route: {err}"));
        }
    }

    pub(crate) fn find(&self, method: &str, path: &str) -> Lookup {
        let path = self.normalize_path(path);
        let matched = Method::try_from(method)
            .ok()
            .and_then(|method| self.table.get(&method))
            .and_then(|routes| routes.at(path).ok());
        if let Some(matched) = matched {
            let params = matched
                .params
                .iter()
                .map(|(name, value)| {
                    let decoded = percent_decode_str(value).decode_utf8_lossy().into_owned();
                    (name.to_string(), decoded)
                })
                .collect();
            return Lookup::Found {
                handler: Arc::clone(matched.value),
                params,
            };
        }

        let mut allow = Method::ALL
            .into_iter()
            .filter(|method| {
                self.table
                    .get(method)
                    .is_some_and(|routes| routes.at(path).is_ok())
            })
            .map(Method::as_str)
            .collect::<Vec<_>>();
        if allow.is_empty() {
            return Lookup::NotFound;
        }
        // Every known path answers OPTIONS automatically.
        allow.push(Method::OPTIONS.as_str());
        Lookup::WrongMethod {
            allow: allow.join(", "),
        }
    }

    fn normalize_path<'a>(&self, path: &'a str) -> &'a str {
        if self.strict_trailing_slash || path == "/" {
            path
        } else {
            path.trim_end_matches("/")
        }
    }
}
