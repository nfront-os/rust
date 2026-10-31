// HTTP on 9front: a hyper server (HTTP/1.1 and HTTP/2, hyper-util's auto builder) on
// 127.0.0.1, its clients hyper (http1 and http2 handshakes over tokio's TcpStream),
// hyper-util's legacy client (HttpConnector, a pool, keep-alive) and reqwest (GET, POST of
// 1 MiB, JSON, a streamed body, 404, a timeout, 20 requests at once).
use std::convert::Infallible;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Full, StreamBody};
use hyper::body::{Frame, Incoming};
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::{TcpListener, TcpStream};

type Body = http_body_util::combinators::BoxBody<Bytes, Infallible>;

fn full(b: impl Into<Bytes>) -> Body {
    Full::new(b.into()).boxed()
}

fn check(name: &str, ok: bool) {
    if ok {
        println!("http: {name} ok");
    } else {
        println!("http: FAIL {name}");
        std::process::exit(1);
    }
}

fn data(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i * 7 % 251) as u8).collect()
}

async fn serve(req: Request<Incoming>) -> Result<Response<Body>, Infallible> {
    let r = match (req.method().as_str(), req.uri().path()) {
        ("GET", "/hello") => Response::new(full("hello")),
        ("POST", "/echo") => {
            let b = req.into_body().collect().await.unwrap().to_bytes();
            Response::new(full(b))
        }
        ("GET", "/json") => {
            let mut r = Response::new(full(r#"{"os":"9front","n":42}"#));
            r.headers_mut().insert("content-type", "application/json".parse().unwrap());
            r
        }
        ("GET", "/chunked") => {
            // 64 chunks of 4 KiB, no content length
            let s = futures_util::stream::iter((0..64).map(|i| {
                Ok::<_, Infallible>(Frame::data(Bytes::from(vec![i as u8; 4096])))
            }));
            Response::new(StreamBody::new(s).boxed())
        }
        ("GET", "/slow") => {
            tokio::time::sleep(Duration::from_secs(5)).await;
            Response::new(full("late"))
        }
        _ => {
            let mut r = Response::new(full("not found"));
            *r.status_mut() = StatusCode::NOT_FOUND;
            r
        }
    };
    Ok(r)
}

#[tokio::main]
async fn main() {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (s, _) = l.accept().await.unwrap();
            tokio::spawn(async move {
                let _ = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new())
                    .serve_connection(TokioIo::new(s), hyper::service::service_fn(serve))
                    .await;
            });
        }
    });
    let base = format!("http://{addr}");

    // hyper, HTTP/1.1
    {
        let s = TcpStream::connect(addr).await.unwrap();
        let (mut tx, conn) = hyper::client::conn::http1::handshake(TokioIo::new(s)).await.unwrap();
        tokio::spawn(conn);
        let r = tx.send_request(Request::get("/hello").header("host", "x").body(full("")).unwrap())
            .await
            .unwrap();
        let b = r.into_body().collect().await.unwrap().to_bytes();
        check("hyper http1 get", &b[..] == b"hello");
        let d = data(1 << 20);
        let r = tx.send_request(Request::post("/echo").header("host", "x").body(full(d.clone())).unwrap())
            .await
            .unwrap();
        let b = r.into_body().collect().await.unwrap().to_bytes();
        check("hyper http1 post 1 MiB", b[..] == d[..]);
    }

    // hyper, HTTP/2 (prior knowledge)
    {
        let s = TcpStream::connect(addr).await.unwrap();
        let (tx, conn) = hyper::client::conn::http2::handshake(TokioExecutor::new(), TokioIo::new(s))
            .await
            .unwrap();
        tokio::spawn(conn);
        let mut v = Vec::new();
        for i in 0..10 {
            let mut tx = tx.clone();
            v.push(tokio::spawn(async move {
                let d = data(10000 + i);
                let r = tx
                    .send_request(Request::post(format!("http://x/echo")).body(full(d.clone())).unwrap())
                    .await
                    .unwrap();
                r.into_body().collect().await.unwrap().to_bytes()[..] == d[..]
            }));
        }
        let mut ok = true;
        for t in v {
            ok &= t.await.unwrap();
        }
        check("hyper http2 10 streams", ok);
    }

    // hyper-util's legacy client: connector, pool
    {
        let c = hyper_util::client::legacy::Client::builder(TokioExecutor::new())
            .build_http::<Body>();
        let mut ok = true;
        for _ in 0..5 {
            let r = c.get(format!("{base}/hello").parse().unwrap()).await.unwrap();
            ok &= r.into_body().collect().await.unwrap().to_bytes()[..] == b"hello"[..];
        }
        check("hyper-util client", ok);
    }

    // reqwest
    let c = reqwest::Client::builder().timeout(Duration::from_secs(20)).build().unwrap();
    let t = c.get(format!("{base}/hello")).send().await.unwrap().text().await.unwrap();
    check("reqwest get", t == "hello");
    let d = data(1 << 20);
    let b = c.post(format!("{base}/echo")).body(d.clone()).send().await.unwrap().bytes().await.unwrap();
    check("reqwest post 1 MiB", b[..] == d[..]);
    let j: serde_json::Value = c.get(format!("{base}/json")).send().await.unwrap().json().await.unwrap();
    check("reqwest json", j["os"] == "9front" && j["n"] == 42);
    let b = c.get(format!("{base}/chunked")).send().await.unwrap().bytes().await.unwrap();
    check("reqwest chunked", b.len() == 64 * 4096 && b[4096 * 63] == 63);
    let r = c.get(format!("{base}/nope")).send().await.unwrap();
    check("reqwest 404", r.status() == StatusCode::NOT_FOUND);
    let r = c.get(format!("{base}/slow")).timeout(Duration::from_millis(300)).send().await;
    check("reqwest timeout", matches!(r, Err(e) if e.is_timeout()));
    let mut v = Vec::new();
    for i in 0..20 {
        let c = c.clone();
        let u = format!("{base}/echo");
        v.push(tokio::spawn(async move {
            let d = data(5000 * (i + 1));
            c.post(u).body(d.clone()).send().await.unwrap().bytes().await.unwrap()[..] == d[..]
        }));
    }
    let mut ok = true;
    for t in v {
        ok &= t.await.unwrap();
    }
    check("reqwest 20 at once", ok);
    let h2 = reqwest::Client::builder().http2_prior_knowledge().build().unwrap();
    let r = h2.get(format!("{base}/hello")).send().await.unwrap();
    check("reqwest http2", r.version() == reqwest::Version::HTTP_2 && r.text().await.unwrap() == "hello");
    println!("http: done");
}
