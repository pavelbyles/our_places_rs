//! A tiny local https server with an `rcgen` CA, and a fixed resolver, for
//! feed fetch and sync tests. Never real DNS, never a real channel. Test
//! support only: built under `cfg(test)` or the `test-support` feature.

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::crypto::aws_lc_rs;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

/// Names the test certificate is valid for.
pub const TEST_HOSTS: &[&str] = &["localhost", "feeds.test", "internal.test", "other.test"];

/// What the server answers for a path.
#[derive(Clone, Debug)]
pub enum Reply {
    /// `200` with a body and `Content-Length`.
    Ok(Vec<u8>),
    /// A status with an empty body.
    Status(u16),
    /// `302` to `location`.
    Redirect(String),
    /// `200` streaming `n` bytes with no `Content-Length`.
    Stream(usize),
    /// Accepts and never answers.
    Stall,
}

/// One request the server saw.
#[derive(Clone, Debug)]
pub struct Seen {
    /// Request path and query.
    pub path: String,
    /// Header lines, lower-cased names.
    pub headers: HashMap<String, String>,
}

/// A running server.
pub struct TestServer {
    /// Its port on 127.0.0.1.
    pub port: u16,
    /// PEM of the CA that signed its certificate.
    pub ca_pem: String,
    /// Requests seen so far.
    pub seen: Arc<Mutex<Vec<Seen>>>,
}

impl TestServer {
    /// The CA as a reqwest root.
    pub fn ca(&self) -> reqwest::Certificate {
        reqwest::Certificate::from_pem(self.ca_pem.as_bytes()).expect("test CA")
    }

    /// `https://<host>:<port><path>`.
    pub fn url(&self, host: &str, path: &str) -> url::Url {
        url::Url::parse(&format!("https://{host}:{}{path}", self.port)).expect("url")
    }

    /// Requests seen so far.
    pub fn requests(&self) -> Vec<Seen> {
        self.seen.lock().expect("seen lock").clone()
    }
}

/// The routing function: path (with query) to reply.
pub type Router = Arc<dyn Fn(&str) -> Reply + Send + Sync>;

/// Starts a server answering with `router`.
pub async fn start(router: Router) -> TestServer {
    let mut ca_params = rcgen::CertificateParams::new(Vec::<String>::new()).expect("ca params");
    ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    let ca_key = rcgen::KeyPair::generate().expect("ca key");
    let ca = rcgen::CertifiedIssuer::self_signed(ca_params, ca_key).expect("ca cert");

    let mut sans: Vec<String> = TEST_HOSTS.iter().map(|h| (*h).to_string()).collect();
    sans.push("127.0.0.1".into());
    let leaf_params = rcgen::CertificateParams::new(sans).expect("leaf params");
    let leaf_key = rcgen::KeyPair::generate().expect("leaf key");
    let leaf = leaf_params.signed_by(&leaf_key, &ca).expect("leaf cert");

    let config = ServerConfig::builder_with_provider(Arc::new(aws_lc_rs::default_provider()))
        .with_safe_default_protocol_versions()
        .expect("tls versions")
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(leaf.der().to_vec())],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_key.serialize_der())),
        )
        .expect("server config");
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let seen = Arc::new(Mutex::new(Vec::new()));

    let seen_task = seen.clone();
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let acceptor = acceptor.clone();
            let router = router.clone();
            let seen = seen_task.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(tcp).await else {
                    return;
                };
                let Some(request) = read_head(&mut tls).await else {
                    return;
                };
                seen.lock().expect("seen lock").push(request.clone());
                let _ = respond(&mut tls, router(&request.path)).await;
            });
        }
    });
    TestServer {
        port,
        ca_pem: ca.pem(),
        seen,
    }
}

async fn read_head<S: AsyncReadExt + Unpin>(stream: &mut S) -> Option<Seen> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1024];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = stream.read(&mut byte).await.ok()?;
        if n == 0 || buf.len() > 64 * 1024 {
            return None;
        }
        buf.extend_from_slice(&byte[..n]);
    }
    let text = String::from_utf8_lossy(&buf).into_owned();
    let mut lines = text.split("\r\n");
    let path = lines.next()?.split(' ').nth(1)?.to_string();
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    Some(Seen { path, headers })
}

async fn respond<S: AsyncWriteExt + Unpin>(stream: &mut S, reply: Reply) -> std::io::Result<()> {
    match reply {
        Reply::Ok(body) => {
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/calendar\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(head.as_bytes()).await?;
            stream.write_all(&body).await?;
        }
        Reply::Status(code) => {
            let head =
                format!("HTTP/1.1 {code} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            stream.write_all(head.as_bytes()).await?;
        }
        Reply::Redirect(location) => {
            let head = format!(
                "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(head.as_bytes()).await?;
        }
        Reply::Stream(n) => {
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/calendar\r\nConnection: close\r\n\r\n",
                )
                .await?;
            let chunk = vec![b'x'; 64 * 1024];
            let mut left = n;
            while left > 0 {
                let take = left.min(chunk.len());
                stream.write_all(&chunk[..take]).await?;
                left -= take;
            }
        }
        Reply::Stall => tokio::time::sleep(Duration::from_secs(30)).await,
    }
    stream.flush().await?;
    stream.shutdown().await
}

/// Resolves names from a fixed map; anything else fails.
pub struct FixedResolver(
    /// Name to address.
    pub HashMap<String, IpAddr>,
);

impl FixedResolver {
    /// A resolver mapping each `(name, ip)`.
    pub fn with(entries: &[(&str, &str)]) -> Arc<dyn Resolve> {
        Arc::new(FixedResolver(
            entries
                .iter()
                .map(|(n, ip)| ((*n).to_string(), ip.parse().expect("ip")))
                .collect(),
        ))
    }
}

impl Resolve for FixedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let found = self.0.get(name.as_str()).copied();
        Box::pin(async move {
            match found {
                Some(ip) => Ok(Box::new(std::iter::once(SocketAddr::new(ip, 0))) as Addrs),
                None => Err("unknown test host".into()),
            }
        })
    }
}
