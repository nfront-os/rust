// tokio on 9front: a multi-thread runtime, an echo server and 20 clients at once, sleep,
// timeout, a channel, tokio::fs.
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (mut s, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let (mut r, mut w) = s.split();
                let n = tokio::io::copy(&mut r, &mut w).await.unwrap();
                if n != 64 * 1024 {
                    eprintln!("tokioecho: server copied {n} of {}", 64 * 1024);
                }
            });
        }
    });

    let mut tasks = Vec::new();
    for i in 0..20u8 {
        tasks.push(tokio::spawn(async move {
            let mut c = TcpStream::connect(addr).await.unwrap();
            let data = vec![i; 64 * 1024];
            let (mut r, mut w) = c.split();
            let write = async {
                w.write_all(&data).await.unwrap();
                w.shutdown().await.ok();
            };
            let mut back = vec![0u8; data.len()];
            let read = async {
                let mut got = 0;
                while got < back.len() {
                    match r.read(&mut back[got..]).await {
                        Ok(0) => break,
                        Ok(n) => got += n,
                        Err(e) => panic!("client {i}: read: {e} after {got}"),
                    }
                }
                got
            };
            let ((), got) = tokio::join!(write, read);
            assert!(got == data.len(), "client {i}: eof after {got} of {}", data.len());
            assert_eq!(back, data);
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    println!("tokioecho: 20 clients echoed");

    let t = Instant::now();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let slept = t.elapsed();
    assert!(slept >= Duration::from_millis(300) && slept < Duration::from_secs(3), "{slept:?}");
    let r = tokio::time::timeout(Duration::from_millis(100), std::future::pending::<()>()).await;
    assert!(r.is_err());
    println!("tokioecho: sleep {slept:?}, timeout ok");

    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    tokio::spawn(async move {
        for i in 0..10 {
            tx.send(i).await.unwrap();
        }
    });
    let mut sum = 0;
    while let Some(i) = rx.recv().await {
        sum += i;
    }
    assert_eq!(sum, 45);

    tokio::fs::write("/tmp/tokioecho.txt", b"from tokio").await.unwrap();
    let s = tokio::fs::read_to_string("/tmp/tokioecho.txt").await.unwrap();
    assert_eq!(s, "from tokio");
    println!("tokioecho: channel, fs ok");
}
