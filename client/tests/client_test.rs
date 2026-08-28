use std::{sync::Arc, time::Duration};

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
use yam_client::client::{Error, HttpClient, HttpClientConfig};

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

    let result = client.get("/").await;

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

    let response = client.get("/").await.expect("HTTPS request should succeed");

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

    let result = client.get("/").await;

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

    let result = client.get("/").await;

    server.abort();
    assert!(matches!(result, Err(Error::Timeout)));
}
