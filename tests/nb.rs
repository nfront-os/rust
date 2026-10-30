// Non-blocking sockets on 9front (std's pumps, os::plan9::net): WouldBlock, connect in the
// background, readiness and its hook, peek, a 1 MiB transfer both ways in one loop (no proc
// of the program blocks), shutdown(Read), UDP, and the pumps' procs gone after the close.
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpListener, UdpSocket};
use std::os::plan9::io::AsRawFd;
use std::os::plan9::net::{connect_nonblocking, readiness, set_ready_hook};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

fn check(name: &str, ok: bool) {
    if ok {
        println!("nb: {name} ok");
    } else {
        println!("nb: FAIL {name}");
        std::process::exit(1);
    }
}

fn until(what: &str, mut f: impl FnMut() -> bool) {
    let t = Instant::now();
    while !f() {
        if t.elapsed() > Duration::from_secs(20) {
            check(what, false);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// The procs of this program (its name in /proc/n/status).
fn procs() -> usize {
    let me = std::fs::read_to_string(format!("/proc/{}/status", std::process::id())).unwrap();
    let name = me.split_whitespace().next().unwrap().to_string();
    std::fs::read_dir("/proc")
        .unwrap()
        .filter_map(|e| std::fs::read_to_string(e.ok()?.path().join("status")).ok())
        .filter(|s| s.split_whitespace().next() == Some(&name))
        .count()
}

fn main() {
    let base = procs();
    {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.set_nonblocking(true).unwrap();
        check("accept wouldblock", matches!(l.accept(), Err(e) if e.kind() == ErrorKind::WouldBlock));
        let addr = l.local_addr().unwrap();

        let mut c = connect_nonblocking(&addr).unwrap();
        let hooked = Arc::new(AtomicUsize::new(0));
        let h = hooked.clone();
        set_ready_hook(c.as_raw_fd(), Some(Arc::new(move || {
            h.fetch_add(1, Ordering::SeqCst);
        })))
        .unwrap();
        until("connected", || readiness(c.as_raw_fd()).writable);
        check("hook", hooked.load(Ordering::SeqCst) > 0);
        check("no error", c.take_error().unwrap().is_none());

        let mut s = loop {
            match l.accept() {
                Ok((s, _)) => break s,
                Err(e) if e.kind() == ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(1)),
                Err(e) => panic!("accept: {e}"),
            }
        };
        s.set_nonblocking(true).unwrap();
        let mut b = [0u8; 16];
        check("read wouldblock", matches!(s.read(&mut b), Err(e) if e.kind() == ErrorKind::WouldBlock));
        check("not readable", !readiness(s.as_raw_fd()).readable);

        c.write_all(b"hello").unwrap();
        until("readable", || readiness(s.as_raw_fd()).readable);
        let n = s.peek(&mut b).unwrap();
        check("peek", &b[..n] == b"hello");
        let n = s.read(&mut b).unwrap();
        check("read", &b[..n] == b"hello");

        // 1 MiB each way, one loop, both ends non-blocking
        let total = 1 << 20;
        let data: Vec<u8> = (0..total).map(|i| (i * 7 % 251) as u8).collect();
        let (mut sent_c, mut sent_s, mut got_c, mut got_s) = (0, 0, Vec::new(), Vec::new());
        let mut buf = vec![0u8; 32 * 1024];
        let t = Instant::now();
        while got_c.len() < total || got_s.len() < total {
            if t.elapsed() > Duration::from_secs(60) {
                check("transfer in time", false);
            }
            for (w, sent) in [(&mut c, &mut sent_c), (&mut s, &mut sent_s)] {
                if *sent < total {
                    match w.write(&data[*sent..]) {
                        Ok(n) => *sent += n,
                        Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                        Err(e) => panic!("write: {e}"),
                    }
                }
            }
            for (r, got) in [(&mut s, &mut got_s), (&mut c, &mut got_c)] {
                match r.read(&mut buf) {
                    Ok(0) => panic!("eof"),
                    Ok(n) => got.extend_from_slice(&buf[..n]),
                    Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                    Err(e) => panic!("read: {e}"),
                }
            }
        }
        check("transfer", got_c == data && got_s == data);

        // shutdown(Read) wakes a blocked reader at once
        s.set_nonblocking(false).unwrap();
        let s2 = s.try_clone().unwrap();
        let t = std::thread::spawn(move || {
            let mut s2 = s2;
            let mut b = [0u8; 4];
            s2.read(&mut b)
        });
        std::thread::sleep(Duration::from_millis(300));
        s.shutdown(Shutdown::Read).unwrap();
        check("shutdown read", matches!(t.join().unwrap(), Ok(0)));

        // a refused connect: readable with the error
        drop(l);
        let r = connect_nonblocking(&addr).unwrap();
        until("refused", || readiness(r.as_raw_fd()).error);
        check("refused error", r.take_error().unwrap().is_some());

        // UDP
        let u = UdpSocket::bind("127.0.0.1:0").unwrap();
        u.set_nonblocking(true).unwrap();
        check("udp wouldblock", matches!(u.recv_from(&mut b), Err(e) if e.kind() == ErrorKind::WouldBlock));
        let v = UdpSocket::bind("127.0.0.1:0").unwrap();
        v.send_to(b"dgram", u.local_addr().unwrap()).unwrap();
        until("udp readable", || readiness(u.as_raw_fd()).readable);
        let (n, from) = u.peek_from(&mut b).unwrap();
        check("udp peek", &b[..n] == b"dgram" && from == v.local_addr().unwrap());
        let (n, _) = u.recv_from(&mut b).unwrap();
        check("udp recv", &b[..n] == b"dgram");
    }
    // every pump's procs end with its sockets
    until("procs gone", || procs() <= base);
    println!("nb: done");
}
