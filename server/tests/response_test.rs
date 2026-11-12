use serde::Serialize;
use yam_server::HttpError;
use yam_server::response::{IntoResponse, Response, ResponseWriter, StatusCode};
#[derive(Serialize)]
struct User {
    name: String,
    email: String,
}
#[tokio::test]
async fn should_send_plain_text_response() {
    let mut output = Vec::new();

    let response = Response::new()
        .status(StatusCode::Ok)
        .set("content-type", "text/plain")
        .send("Hello");
    ResponseWriter::new(&mut output)
        .send_response(response)
        .await
        .expect("res should be sent successfully.");

    let response = String::from_utf8(output).expect("response should be converted to string");

    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(response.contains("content-type: text/plain\r\n"));
    assert!(response.contains("content-length: 5\r\n"));
    assert!(response.ends_with("\r\n\r\nHello"));
}

#[tokio::test]
async fn should_send_head_response_without_body() {
    let mut output = Vec::new();

    ResponseWriter::new(&mut output)
        .head_only(true)
        .send_response(Response::new().send("Hello"))
        .await
        .expect("res should be sent successfully.");

    let response = String::from_utf8(output).expect("response should be converted to string");

    assert!(response.contains("content-length: 5\r\n"));
    assert!(response.ends_with("\r\n\r\n"));
}

#[tokio::test]
async fn should_send_json_response() {
    let mut output = Vec::new();
    let user = User {
        name: "john doe".to_string(),
        email: "john@example.com".to_string(),
    };
    let response = Response::new().json(&user);
    ResponseWriter::new(&mut output)
        .send_response(response)
        .await
        .expect("response to be sent");
    let response = String::from_utf8(output).expect("failed to convert to a string");
    println!("response {:?}", response);
    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(response.contains("content-type: application/json"));
    assert!(response.contains("content-length: 46\r\n"));
    assert!(response.ends_with("\r\n\r\n{\"name\":\"john doe\",\"email\":\"john@example.com\"}"));
}

#[tokio::test]
async fn should_send_binary_body() {
    let mut output = Vec::new();
    let response = Response::new().send(vec![1, 2, 3, 4]);
    ResponseWriter::new(&mut output)
        .send_response(response)
        .await
        .expect("binary response should be sent");

    assert!(output.starts_with(b"HTTP/1.1 200 OK\r\n"));
    assert!(
        output
            .windows(b"content-length: 4\r\n".len())
            .any(|window| window == b"content-length: 4\r\n")
    );
    assert!(output.ends_with(&[b'\r', b'\n', b'\r', b'\n', 1, 2, 3, 4]));
}

#[tokio::test]
async fn should_send_not_found_status() {
    let mut output = Vec::new();
    let response = Response::new()
        .status(StatusCode::NotFound)
        .send("Not found");
    ResponseWriter::new(&mut output)
        .send_response(response)
        .await
        .expect("not found response should be sent");

    let response = String::from_utf8(output).expect("response should be valid utf-8");

    assert!(response.starts_with("HTTP/1.1 404 Not Found\r\n"));
    assert!(response.contains("content-length: 9\r\n"));
    assert!(response.ends_with("\r\n\r\nNot found"));
}

#[tokio::test]
async fn should_convert_http_error_to_response() {
    let err = serde_json::from_str::<serde_json::Value>("{bad").unwrap_err();
    let response = HttpError::Json(err).into_response();

    assert_eq!(response.status, StatusCode::BadRequest);
    assert!(!response.body.is_empty());
}

#[test]
fn should_map_specific_request_errors_to_statuses() {
    let cases = [
        (
            yam_server::request::Error::RequestTooLarge,
            StatusCode::ContentTooLarge,
        ),
        (
            yam_server::request::Error::MethodNotAllowed,
            StatusCode::NotImplemented,
        ),
        (
            yam_server::request::Error::Parse(
                yam_server::request::ParseError::UnsupportedHttpVersion,
            ),
            StatusCode::HttpVersionNotSupported,
        ),
    ];

    for (error, expected_status) in cases {
        assert_eq!(HttpError::Request(error).status(), expected_status);
    }
}

#[tokio::test]
async fn should_send_redirect_response() {
    let mut output = Vec::new();
    let response = Response::redirect("/users/1").status(StatusCode::SeeOther);
    ResponseWriter::new(&mut output)
        .send_response(response)
        .await
        .expect("res should be sent successfully.");

    let response = String::from_utf8(output).expect("response should be converted to string");

    assert!(response.starts_with("HTTP/1.1 303 See Other\r\n"));
    assert!(response.contains("location: /users/1\r\n"));
}
