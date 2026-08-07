use yam_server::response::{Response, StatusCode};
use serde::Serialize;
#[derive(Serialize)]
struct User {
    name: String,
    email: String,
}
#[tokio::test]
async fn should_send_plain_text_response() {
    let mut output = Vec::new();

    Response::new(&mut output)
        .status(StatusCode::StatusOk)
        .set("content-type", "text/plain")
        .send("Hello")
        .await
        .expect("res should be sent successfully.");

    let response = String::from_utf8(output).expect("response should be converted to string");

    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(response.contains("content-type: text/plain\r\n"));
    assert!(response.contains("content-length: 5\r\n"));
    assert!(response.ends_with("\r\n\r\nHello"));
}

#[tokio::test]
async fn should_send_json_response() {
    let mut output = Vec::new();
    let user = User {
        name: "john doe".to_string(),
        email: "john@example.com".to_string(),
    };
    Response::new(&mut output)
        .status(StatusCode::StatusOk)
        .set("content-type", "application/json")
        .json(&user)
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
    Response::new(&mut output)
        .set("content-type", "application/octet-stream")
        .send(vec![1, 2, 3, 4])
        .await
        .expect("binary response should be sent");

    assert!(output.starts_with(b"HTTP/1.1 200 OK\r\n"));
    assert!(
        output
            .windows(b"content-type: application/octet-stream\r\n".len())
            .any(|window| window == b"content-type: application/octet-stream\r\n")
    );
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
    Response::new(&mut output)
        .status(StatusCode::StatusNotFound)
        .send("Not found")
        .await
        .expect("not found response should be sent");

    let response = String::from_utf8(output).expect("response should be valid utf-8");

    assert!(response.starts_with("HTTP/1.1 404 Not Found\r\n"));
    assert!(response.contains("content-length: 9\r\n"));
    assert!(response.ends_with("\r\n\r\nNot found"));
}
