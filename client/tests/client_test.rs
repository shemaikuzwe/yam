use std::{sync::Arc, time::Duration};

use futures::stream;
use rcgen::{CertifiedKey, generate_simple_self_signed};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use tokio_rustls::{
    TlsAcceptor,
    rustls::{
        ClientConfig, RootCertStore, ServerConfig,
        pki_types::{CertificateDer, PrivatePkcs8KeyDer},
    },
};
use yam_client::{Body, Error, HttpClient, HttpClientConfig, RequestOptions};

fn tls_configuration() -> (TlsAcceptor, CertificateDer<'static>) {
    let CertifiedKey { cert, signing_key } = generate_simple_self_signed(vec!["127.0.0.1".into()])
        .expect("test certificate should generate");
    let certificate = cert.der().clone();
    let private_key = PrivatePkcs8KeyDer::from(signing_key.serialize_der());
    let configuration = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![certificate.clone()], private_key.into())
        .expect("TLS server configuration should be valid");

    (TlsAcceptor::from(Arc::new(configuration)), certificate)
}

fn client_tls_configuration(certificate: CertificateDer<'static>) -> ClientConfig {
    let mut roots = RootCertStore::empty();
    roots
        .add(certificate)
        .expect("test certificate should be trusted");

    ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth()
}

#[tokio::test]
async fn should_timeout_request() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let address = listener.local_addr().expect("listener should have address");
    let server = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.expect("server should accept");
        std::future::pending::<()>().await;
    });
    let client = HttpClient::new(HttpClientConfig {
        base_url: Some(format!("http://{address}")),
        timeout: Some(Duration::from_millis(50)),
        ..Default::default()
    });

    let result = client.get("/", RequestOptions::default()).await;

    server.abort();
    assert!(matches!(result, Err(Error::Timeout)));
}

#[tokio::test]
async fn should_perform_tls_handshake() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let address = listener.local_addr().expect("listener should have address");
    let (acceptor, certificate) = tls_configuration();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("server should accept");
        let mut stream = acceptor
            .accept(stream)
            .await
            .expect("TLS handshake should succeed");
        let mut request = [0; 1024];
        stream
            .read(&mut request)
            .await
            .expect("server should read request");
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 2\r\n\r\nok")
            .await
            .expect("server should write response");
        stream.shutdown().await.expect("server should close");
    });
    let client = HttpClient::new(HttpClientConfig {
        base_url: Some(format!("https://{address}")),
        tls_configuration: Some(client_tls_configuration(certificate)),
        ..Default::default()
    });

    let response = client
        .get("/", RequestOptions::default())
        .await
        .expect("HTTPS request should succeed");

    server.await.expect("server task should finish");
    assert_eq!(response.status, 200);
    assert_eq!(response.text().expect("response should be UTF-8"), "ok");
}

#[tokio::test]
async fn should_reject_untrusted_certificate() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let address = listener.local_addr().expect("listener should have address");
    let (acceptor, _) = tls_configuration();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("server should accept");
        let _ = acceptor.accept(stream).await;
    });
    let client = HttpClient::new(HttpClientConfig {
        base_url: Some(format!("https://{address}")),
        ..Default::default()
    });

    let result = client.get("/", RequestOptions::default()).await;

    server.await.expect("server task should finish");
    assert!(matches!(result, Err(Error::Io(_))));
}

#[tokio::test]
async fn should_timeout_tls_handshake() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let address = listener.local_addr().expect("listener should have address");
    let server = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.expect("server should accept");
        std::future::pending::<()>().await;
    });
    let client = HttpClient::new(HttpClientConfig {
        base_url: Some(format!("https://{address}")),
        timeout: Some(Duration::from_millis(50)),
        ..Default::default()
    });

    let result = client.get("/", RequestOptions::default()).await;

    server.abort();
    assert!(matches!(result, Err(Error::Timeout)));
}

#[tokio::test]
async fn should_merge_request_headers_in_precedence_order() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let address = listener.local_addr().expect("listener should have address");
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("server should accept");
        let mut request = [0; 2048];
        let read = stream
            .read(&mut request)
            .await
            .expect("server should read request");
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 2\r\n\r\nok")
            .await
            .expect("server should write response");
        String::from_utf8(request[..read].to_vec()).expect("request should be UTF-8")
    });
    let client = HttpClient::new(HttpClientConfig {
        base_url: Some(format!("http://{address}")),
        headers: vec![
            ("accept".into(), "text/plain".into()),
            ("accept".into(), "application/xml".into()),
            ("content-type".into(), "text/plain".into()),
            ("host".into(), "invalid.example".into()),
        ],
        ..Default::default()
    });
    let options = RequestOptions {
        headers: vec![("accept".into(), "application/json".into())],
        body: Some(Body::json(&serde_json::json!({ "name": "Yam" })).unwrap()),
    };

    client
        .post("/users", options)
        .await
        .expect("request should succeed");
    let request = server.await.expect("server task should finish");

    assert!(request.contains("accept: application/json\r\n"));
    assert!(!request.contains("accept: text/plain\r\n"));
    assert!(!request.contains("accept: application/xml\r\n"));
    assert!(request.contains("content-type: application/json\r\n"));
    assert!(request.contains(&format!("host: {address}\r\n")));
    assert!(request.contains("content-length: 14\r\n"));
    assert!(request.ends_with("\r\n\r\n{\"name\":\"Yam\"}"));
}

#[tokio::test]
async fn should_send_streamed_request_as_chunked() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let address = listener.local_addr().expect("listener should have address");
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("server should accept");
        let mut request = Vec::new();
        let mut buffer = [0; 1024];
        while !request.ends_with(b"\r\n0\r\n\r\n") {
            let read = stream
                .read(&mut buffer)
                .await
                .expect("server should read request");
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
        }
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 2\r\n\r\nok")
            .await
            .expect("server should write response");
        String::from_utf8(request).expect("request should be UTF-8")
    });
    let client = HttpClient::new(HttpClientConfig {
        base_url: Some(format!("http://{address}")),
        ..Default::default()
    });
    let options = RequestOptions {
        headers: vec![("content-type".into(), "text/plain".into())],
        body: Some(Body::stream(stream::iter([
            b"hello".to_vec(),
            b" world".to_vec(),
        ]))),
    };

    let response = client
        .post("/upload", options)
        .await
        .expect("request should succeed");
    let request = server.await.expect("server task should finish");

    assert_eq!(response.text().expect("response should be UTF-8"), "ok");
    assert!(request.contains("transfer-encoding: chunked\r\n"));
    assert!(!request.contains("content-length:"));
    assert!(request.ends_with("\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n"));
}

#[tokio::test]
async fn should_decode_chunked_response() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let address = listener.local_addr().expect("listener should have address");
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("server should accept");
        let mut request = [0; 1024];
        let read = stream
            .read(&mut request)
            .await
            .expect("server should read request");
        assert!(read > 0);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n")
            .await
            .expect("server should write response");
        stream
            .write_all(b"1\r\n \r\n5\r\nworld\r\n0\r\n\r\n")
            .await
            .expect("server should write response");
    });
    let client = HttpClient::new(HttpClientConfig {
        base_url: Some(format!("http://{address}")),
        ..Default::default()
    });

    let response = client
        .get("/stream", RequestOptions::default())
        .await
        .expect("request should succeed");

    server.await.expect("server task should finish");
    assert_eq!(
        response.text().expect("response should be UTF-8"),
        "hello world"
    );
}
