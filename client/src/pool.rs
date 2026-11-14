use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
};

use deadpool::managed;
use tokio::net::TcpStream;
use tokio_rustls::{
    TlsConnector,
    client::TlsStream,
    rustls::{ClientConfig, pki_types::ServerName},
};
use url::Url;

use crate::error::Error;

const READ_BUFFER_SIZE: usize = 8 * 1024;

pub(crate) enum Stream {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}

pub(crate) struct Conn {
    pub(crate) stream: Stream,
    pub(crate) reusable: bool,
    pub(crate) buf: Vec<u8>,
}

pub(crate) struct ConnManager {
    scheme: String,
    host: String,
    port: u16,
    tls_configuration: Arc<ClientConfig>,
}

impl managed::Manager for ConnManager {
    type Type = Conn;
    type Error = Error;

    async fn create(&self) -> Result<Conn, Error> {
        let tcp = TcpStream::connect((self.host.as_str(), self.port)).await?;
        // Requests are written in one go, so Nagle would only add latency.
        tcp.set_nodelay(true)?;
        let stream = match self.scheme.as_str() {
            "http" => Stream::Plain(tcp),
            "https" => {
                let server_name = ServerName::try_from(self.host.clone())
                    .map_err(|_| Error::InvalidHostname(self.host.clone()))?;
                let connector = TlsConnector::from(Arc::clone(&self.tls_configuration));
                Stream::Tls(Box::new(connector.connect(server_name, tcp).await?))
            }
            scheme => return Err(Error::UnsupportedScheme(scheme.into())),
        };
        Ok(Conn {
            stream,
            reusable: true,
            buf: Vec::with_capacity(READ_BUFFER_SIZE),
        })
    }

    async fn recycle(
        &self,
        conn: &mut Conn,
        _: &managed::Metrics,
    ) -> managed::RecycleResult<Error> {
        if conn.reusable {
            Ok(())
        } else {
            Err(managed::RecycleError::message("connection is not reusable"))
        }
    }
}

#[derive(Default)]
pub(crate) struct Pools(Mutex<HashMap<String, managed::Pool<ConnManager>>>);

impl fmt::Debug for Pools {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Pools")
    }
}

impl Pools {
    pub(crate) async fn get(
        &self,
        url: &Url,
        tls_configuration: &Arc<ClientConfig>,
    ) -> Result<managed::Object<ConnManager>, Error> {
        self.pool(url, tls_configuration)?
            .get()
            .await
            .map_err(|e| match e {
                managed::PoolError::Backend(e) => e,
                e => Error::Pool(e.to_string()),
            })
    }

    fn pool(
        &self,
        url: &Url,
        tls_configuration: &Arc<ClientConfig>,
    ) -> Result<managed::Pool<ConnManager>, Error> {
        let scheme = url.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(Error::UnsupportedScheme(scheme.into()));
        }
        let host = url
            .host_str()
            .ok_or_else(|| Error::InvalidHostname(url.to_string()))?;
        let port = url
            .port_or_known_default()
            .ok_or_else(|| Error::MissingPort(url.to_string()))?;
        let origin = &url[url::Position::BeforeScheme..url::Position::BeforePath];

        let mut pools = self.0.lock().expect("pool map lock poisoned");
        if let Some(pool) = pools.get(origin) {
            return Ok(pool.clone());
        }
        let manager = ConnManager {
            scheme: scheme.to_owned(),
            host: host.to_owned(),
            port,
            tls_configuration: Arc::clone(tls_configuration),
        };
        let pool = managed::Pool::builder(manager)
            .build()
            .map_err(|e| Error::Pool(e.to_string()))?;
        pools.insert(origin.to_owned(), pool.clone());
        Ok(pool)
    }
}
