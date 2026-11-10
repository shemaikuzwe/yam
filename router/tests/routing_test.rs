use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use yam_router::{Middleware, Next, Router, RouterConfig, middleware::Scope};
use yam_server::{Handler, HandlerFuture, Request, Response, StatusCode};

fn request(method: &str, path: &str) -> Request {
    let mut request = Request::new();
    request
        .parse(format!("{method} {path} HTTP/1.1\r\n\r\n").as_bytes())
        .expect("request should be valid");
    request
}

#[tokio::test]
async fn should_dispatch_registered_get_route() {
    let mut router = Router::new(RouterConfig::default());
    router.get("/users", |_| async { "users" });

    let response = router
        .call(request("GET", "/users"))
        .await
        .expect("handler should succeed");

    assert_eq!(response.status, StatusCode::Ok);
    assert_eq!(response.body, b"users");
}

#[tokio::test]
async fn should_dispatch_supported_http_methods() {
    let mut router = Router::new(RouterConfig::default());
    router.get("/get", |_| async { "GET" });
    router.post("/post", |_| async { "POST" });
    router.put("/put", |_| async { "PUT" });
    router.patch("/patch", |_| async { "PATCH" });
    router.delete("/delete", |_| async { "DELETE" });

    for (method, path) in [
        ("GET", "/get"),
        ("POST", "/post"),
        ("PUT", "/put"),
        ("PATCH", "/patch"),
        ("DELETE", "/delete"),
    ] {
        let response = router
            .call(request(method, path))
            .await
            .expect("handler should succeed");

        assert_eq!(response.status, StatusCode::Ok);
        assert_eq!(response.body, method.as_bytes());
    }
}

#[tokio::test]
async fn should_extract_path_parameters() {
    let mut router = Router::new(RouterConfig::default());
    router.get("/users/{id}", |request| async move {
        request.param("id").unwrap_or("missing").to_string()
    });

    let response = router
        .call(request("GET", "/users/42"))
        .await
        .expect("handler should succeed");

    assert_eq!(response.body, b"42");
}

#[tokio::test]
async fn should_answer_head_with_get_handler() {
    let mut router = Router::new(RouterConfig::default());
    router.get("/users", |_| async { "users" });

    let response = router
        .call(request("HEAD", "/users"))
        .await
        .expect("handler should succeed");

    assert_eq!(response.status, StatusCode::Ok);
}

#[tokio::test]
async fn should_answer_options_with_allowed_methods() {
    let mut router = Router::new(RouterConfig::default());
    router.get("/users", |_| async { "users" });
    router.post("/users", |_| async { "created" });

    let response = router
        .call(request("OPTIONS", "/users"))
        .await
        .expect("router should return a response");

    assert_eq!(response.status, StatusCode::NoContent);
    assert_eq!(
        response.headers.get("allow"),
        Some("GET, HEAD, POST, OPTIONS")
    );
}

#[tokio::test]
async fn should_return_not_found_for_options_on_unknown_path() {
    let router = Router::new(RouterConfig::default());

    let response = router
        .call(request("OPTIONS", "/missing"))
        .await
        .expect("router should return a response");

    assert_eq!(response.status, StatusCode::NotFound);
}

#[tokio::test]
async fn should_run_router_middleware_for_options() {
    let ran = Arc::new(AtomicBool::new(false));
    let mut router = Router::new(RouterConfig::default());
    let ran_in_middleware = Arc::clone(&ran);
    router.middleware(move |request: Request, next: Next| {
        ran_in_middleware.store(true, Ordering::SeqCst);
        next.run(request)
    });
    router.get("/users", |_| async { "users" });

    router
        .call(request("OPTIONS", "/users"))
        .await
        .expect("router should return a response");

    assert!(ran.load(Ordering::SeqCst));
}

#[tokio::test]
async fn should_return_not_found_for_unknown_path() {
    let router = Router::new(RouterConfig::default());

    let response = router
        .call(request("GET", "/missing"))
        .await
        .expect("router should return a response");

    assert_eq!(response.status, StatusCode::NotFound);
}

#[tokio::test]
async fn should_return_method_not_allowed_for_existing_path() {
    let mut router = Router::new(RouterConfig::default());
    router.get("/users", |_| async { "users" });
    router.post("/users", |_| async { "created" });

    let response = router
        .call(request("DELETE", "/users"))
        .await
        .expect("router should return a response");

    assert_eq!(response.status, StatusCode::MethodNotAllowed);
    assert_eq!(
        response.headers.get("allow"),
        Some("GET, HEAD, POST, OPTIONS")
    );
}

#[tokio::test]
async fn should_ignore_trailing_slashes_by_default() {
    let mut router = Router::new(RouterConfig::default());
    router.get("/users", |_| async { "users" });

    for path in ["/users", "/users/", "/users///"] {
        let response = router
            .call(request("GET", path))
            .await
            .expect("handler should succeed");

        assert_eq!(response.status, StatusCode::Ok);
    }
}

#[tokio::test]
async fn should_respect_strict_trailing_slashes() {
    let mut router = Router::new(RouterConfig {
        strict_trailing_slash: true,
        ..RouterConfig::default()
    });
    router.get("/users", |_| async { "users" });

    let response = router
        .call(request("GET", "/users/"))
        .await
        .expect("router should return a response");

    assert_eq!(response.status, StatusCode::NotFound);
}

#[tokio::test]
async fn should_apply_route_prefix() {
    let mut router = Router::new(RouterConfig {
        route_prefix: "/api".to_string(),
        ..RouterConfig::default()
    });
    router.get("/users", |_| async { "users" });

    let response = router
        .call(request("GET", "/api/users"))
        .await
        .expect("handler should succeed");

    assert_eq!(response.status, StatusCode::Ok);
}

#[tokio::test]
async fn should_run_middleware_in_registration_order() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut router = Router::new(RouterConfig::default());

    for name in ["first", "second"] {
        let events = Arc::clone(&events);
        router.middleware(move |request, next: Next| {
            let events = Arc::clone(&events);
            async move {
                events.lock().unwrap().push(format!("{name}:before"));
                let response = next.run(request).await;
                events.lock().unwrap().push(format!("{name}:after"));
                response
            }
        });
    }
    router.get("/users", |_| async { "users" });

    router
        .call(request("GET", "/users"))
        .await
        .expect("handler should succeed");

    assert_eq!(
        *events.lock().unwrap(),
        [
            "first:before",
            "second:before",
            "second:after",
            "first:after"
        ]
    );
}

#[tokio::test]
async fn should_not_run_default_middlewares_for_unmateched_routes() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut router = Router::new(RouterConfig::default());

    let seen_by_middleware = Arc::clone(&seen);
    router.middleware(move |request: Request, next: Next| {
        let seen = Arc::clone(&seen_by_middleware);
        async move {
            let response = next.run(request).await;
            if let Ok(response) = &response {
                seen.lock().unwrap().push(response.status);
            }
            response
        }
    });
    router.get("/users", |_| async { "users" });

    for (method, expected) in [
        ("GET", StatusCode::NotFound),
        ("POST", StatusCode::MethodNotAllowed),
    ] {
        let path = if method == "POST" {
            "/users"
        } else {
            "/missing"
        };
        let response = router
            .call(request(method, path))
            .await
            .expect("router should return a response");

        assert_eq!(response.status, expected);
    }
    assert!((*seen.lock().unwrap()).is_empty());
}

#[tokio::test]
async fn should_run_global_middleware() {
    struct Observer {
        seen: Arc<Mutex<Vec<StatusCode>>>,
    }
    impl Middleware for Observer {
        fn call(&self, req: Request, next: Next) -> HandlerFuture {
            let seen = Arc::clone(&self.seen);
            Box::pin(async move {
                let response = next.run(req).await;
                if let Ok(response) = &response {
                    seen.lock().unwrap().push(response.status);
                }
                response
            })
        }
        fn scope(&self) -> Scope {
            Scope::Global
        }
    }

    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut router = Router::new(RouterConfig::default());
    router.middleware(Observer {
        seen: Arc::clone(&seen),
    });
    router.get("/users", |_| async { "users" });

    let response = router
        .call(request("GET", "/missing"))
        .await
        .expect("router should return a response");

    assert_eq!(response.status, StatusCode::NotFound);
    assert_eq!(*seen.lock().unwrap(), [StatusCode::NotFound]);
}

#[tokio::test]
async fn should_allow_middleware_to_short_circuit_handler() {
    let handler_called = Arc::new(AtomicBool::new(false));
    let mut router = Router::new(RouterConfig::default());
    router.middleware(|_request, _next: Next| async {
        Ok(Response::new()
            .status(StatusCode::Unauthorized)
            .send("Unauthorized"))
    });

    let handler_called_by_route = Arc::clone(&handler_called);
    router.get("/users", move |_| {
        handler_called_by_route.store(true, Ordering::SeqCst);
        async { "users" }
    });

    let response = router
        .call(request("GET", "/users"))
        .await
        .expect("middleware should return a response");

    assert_eq!(response.status, StatusCode::Unauthorized);
    assert!(!handler_called.load(Ordering::SeqCst));
}

#[tokio::test]
async fn should_percent_decode_path_parameters() {
    let mut router = Router::new(RouterConfig::default());
    router.get("/users/{name}", |request| async move {
        request.param("name").unwrap_or_default().to_string()
    });

    let response = router
        .call(request("GET", "/users/john%20doe"))
        .await
        .expect("handler should succeed");

    assert_eq!(response.body, b"john doe");
}

#[tokio::test]
async fn should_preserve_segments_with_encoded_slash() {
    let mut router = Router::new(RouterConfig::default());
    router.get("/files/{path}", |request| async move {
        request.param("path").unwrap_or_default().to_string()
    });

    // %2F should NOT split into multiple segments during routing;
    // matchit sees the raw "nested%2Ffile.txt" as one segment,
    // then we decode it to "nested/file.txt" in the param value.
    let response = router
        .call(request("GET", "/files/nested%2Ffile.txt"))
        .await
        .expect("handler should succeed");

    assert_eq!(response.status, StatusCode::Ok);
    assert_eq!(response.body, b"nested/file.txt");
}

#[tokio::test]
async fn should_decode_multibyte_utf8_in_path_parameters() {
    let mut router = Router::new(RouterConfig::default());
    router.get("/tags/{tag}", |request| async move {
        request.param("tag").unwrap_or_default().to_string()
    });

    // "café" = 63 61 66 C3 A9 → /tags/caf%C3%A9
    let response = router
        .call(request("GET", "/tags/caf%C3%A9"))
        .await
        .expect("handler should succeed");

    assert_eq!(response.body, "café".as_bytes());
}

#[tokio::test]
async fn should_run_path_middleware_only_under_its_prefix() {
    struct Admin {
        seen: Arc<Mutex<Vec<String>>>,
    }
    impl Middleware for Admin {
        fn call(&self, req: Request, next: Next) -> HandlerFuture {
            self.seen.lock().unwrap().push(req.path().to_string());
            next.run(req)
        }
        fn scope(&self) -> Scope {
            Scope::Path("/admin")
        }
    }

    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut router = Router::new(RouterConfig::default());
    router.middleware(Admin {
        seen: Arc::clone(&seen),
    });
    for path in ["/admin", "/admin/users", "/administrator", "/users"] {
        router.get(path, |_| async { "ok" });
    }

    for path in ["/admin", "/admin/users", "/administrator", "/users"] {
        router
            .call(request("GET", path))
            .await
            .expect("handler should succeed");
    }

    assert_eq!(*seen.lock().unwrap(), ["/admin", "/admin/users"]);
}
