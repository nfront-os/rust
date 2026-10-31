// Names and TLS on 9front. DNS: std's and tokio's lookup_host (ndb's /net/cs, ndb/dns
// over QEMU's DNS), localhost, a name that does not exist, 10 lookups at once, reqwest's
// resolver. HTTPS here: a hyper server behind tokio-rustls with a certificate from rcgen;
// reqwest trusting it (HTTP/1.1, HTTP/2 by ALPN, 1 MiB), and not (an unknown issuer). HTTPS
// out: reqwest with the system's roots (/sys/lib/tls/ca.pem) to crates.io.
use std::convert::Infallible;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::{Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::TcpListener;

fn check(name: &str, ok: bool) {
    if ok {
        println!("tlsdns: {name} ok");
    } else {
        println!("tlsdns: FAIL {name}");
        std::process::exit(1);
    }
}

async fn serve(req: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let v = format!("{:?}", req.version());
    let b = req.into_body().collect().await.unwrap().to_bytes();
    if b.is_empty() {
        Ok(Response::new(Full::new(Bytes::from(v))))
    } else {
        Ok(Response::new(Full::new(b)))
    }
}

#[tokio::main]
async fn main() {
    // DNS
    let a: Vec<SocketAddr> = "localhost:80".to_socket_addrs().map(|i| i.collect()).unwrap_or_default();
    println!("tlsdns: localhost: {a:?}");
    check("std localhost", a.iter().any(|a| a.ip().is_loopback()));
    let a: Vec<SocketAddr> = "crates.io:443".to_socket_addrs().unwrap().collect();
    println!("tlsdns: crates.io: {a:?}");
    check("std lookup", !a.is_empty() && a.iter().all(|a| a.port() == 443));
    let a: Vec<SocketAddr> = tokio::net::lookup_host("static.crates.io:80").await.unwrap().collect();
    check("tokio lookup", !a.is_empty());
    // a long name: ndb/cs cut its errors at 63 bytes (9front-gccwt MEMORY.md)
    let e = "no-such-host-abcdefghijklmnopqrstuvwxyz.invalid:80".to_socket_addrs();
    println!("tlsdns: unknown: {:?}", e.as_ref().err());
    check("unknown name", matches!(&e, Err(e) if e.to_string().contains("does not exist")));
    let mut v = Vec::new();
    for h in ["crates.io", "static.crates.io", "index.crates.io", "github.com", "9front.org"]
        .iter()
        .cycle()
        .take(10)
    {
        v.push(tokio::net::lookup_host(format!("{h}:443")));
    }
    let mut ok = true;
    for f in v {
        ok &= f.await.map(|mut i| i.next().is_some()).unwrap_or(false);
    }
    check("10 lookups at once", ok);

    // HTTPS here
    let _ = rustls::crypto::ring::default_provider().install_default();
    let ck = rcgen::generate_simple_self_signed(vec!["test.9front".to_string()]).unwrap();
    let cert = ck.cert.der().clone();
    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(ck.key_pair.serialize_der().into());
    let mut sc = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert.clone()], key)
        .unwrap();
    sc.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(sc));
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (s, _) = l.accept().await.unwrap();
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(s) = acceptor.accept(s).await else { return };
                let _ = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new())
                    .serve_connection(TokioIo::new(s), hyper::service::service_fn(serve))
                    .await;
            });
        }
    });
    let url = format!("https://test.9front:{}/", addr.port());
    let root = reqwest::Certificate::from_der(&cert).unwrap();
    let c = reqwest::Client::builder()
        .use_rustls_tls()
        .add_root_certificate(root.clone())
        .resolve("test.9front", addr)
        .http1_only()
        .build()
        .unwrap();
    let t = c.get(&url).send().await.unwrap().text().await.unwrap();
    check("https http1", t == "HTTP/1.1");
    let d: Vec<u8> = (0..1 << 20).map(|i: usize| (i * 7 % 251) as u8).collect();
    let b = c.post(&url).body(d.clone()).send().await.unwrap().bytes().await.unwrap();
    check("https post 1 MiB", b[..] == d[..]);
    let c2 = reqwest::Client::builder()
        .use_rustls_tls()
        .add_root_certificate(root)
        .resolve("test.9front", addr)
        .build()
        .unwrap();
    let r = c2.get(&url).send().await.unwrap();
    check("https http2 (alpn)", r.version() == reqwest::Version::HTTP_2 && r.text().await.unwrap() == "HTTP/2.0");
    let c3 = reqwest::Client::builder().use_rustls_tls().resolve("test.9front", addr).build().unwrap();
    let e = c3.get(&url).send().await;
    println!("tlsdns: untrusted: {:?}", e.as_ref().err().map(|e| std::error::Error::source(e).map(|s| s.to_string())));
    check("untrusted rejected", e.is_err());

    // HTTPS out
    let c = reqwest::Client::builder()
        .use_rustls_tls()
        .timeout(Duration::from_secs(60))
        .user_agent("9front-rust-test")
        .build()
        .unwrap();
    let r = c.get("https://index.crates.io/config.json").send().await.unwrap();
    let st = r.status();
    let t = r.text().await.unwrap();
    check("https crates.io", st.is_success() && t.contains("\"dl\""));
    let r = c.get("https://crates.io/api/v1/crates/serde").send().await.unwrap();
    check("https crates.io api", r.status().is_success() && r.text().await.unwrap().contains("\"serde\""));
    println!("tlsdns: done");
}
