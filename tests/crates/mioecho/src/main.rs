// mio on 9front: one Poll loop, an echo server and a client (200 KiB), a Waker from a thread.
use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token, Waker};
use std::io::{ErrorKind, Read, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};

const LISTENER: Token = Token(0);
const CLIENT: Token = Token(1);
const SERVER: Token = Token(2);
const WAKE: Token = Token(3);

fn main() {
    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(64);
    let mut listener = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = listener.local_addr().unwrap();
    poll.registry().register(&mut listener, LISTENER, Interest::READABLE).unwrap();
    let mut client = TcpStream::connect(addr).unwrap();
    poll.registry().register(&mut client, CLIENT, Interest::READABLE | Interest::WRITABLE).unwrap();
    let waker = Arc::new(Waker::new(poll.registry(), WAKE).unwrap());
    let w = waker.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        w.wake().unwrap();
    });

    let total = 200 * 1024;
    let data: Vec<u8> = (0..total).map(|i| (i % 253) as u8).collect();
    let (mut server, mut sent, mut back, mut woken) = (None::<TcpStream>, 0, Vec::new(), false);
    let mut echo: Vec<u8> = Vec::new();
    let mut buf = vec![0u8; 16 * 1024];
    let t = Instant::now();
    while back.len() < total || !woken {
        assert!(t.elapsed() < Duration::from_secs(60), "too slow: sent {sent} back {}", back.len());
        poll.poll(&mut events, Some(Duration::from_secs(5))).unwrap();
        for ev in events.iter() {
            match ev.token() {
                LISTENER => loop {
                    match listener.accept() {
                        Ok((mut s, _)) => {
                            poll.registry().register(&mut s, SERVER, Interest::READABLE | Interest::WRITABLE).unwrap();
                            server = Some(s);
                        }
                        Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                        Err(e) => panic!("accept: {e}"),
                    }
                },
                WAKE => woken = true,
                _ => {}
            }
        }
        // client: write what is left, read what came back
        while sent < total {
            match client.write(&data[sent..]) {
                Ok(n) => sent += n,
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => panic!("client write: {e}"),
            }
        }
        loop {
            match client.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => back.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => panic!("client read: {e}"),
            }
        }
        // server: read into echo, write echo out
        if let Some(s) = server.as_mut() {
            loop {
                match s.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => echo.extend_from_slice(&buf[..n]),
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(e) => panic!("server read: {e}"),
                }
            }
            while !echo.is_empty() {
                match s.write(&echo) {
                    Ok(n) => { echo.drain(..n); }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(e) => panic!("server write: {e}"),
                }
            }
        }
    }
    assert_eq!(back, data);
    println!("mioecho: ok ({} KiB echoed, woken)", total / 1024);
}
