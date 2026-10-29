//! A C library built against APE (tests/c/apehello.c), in a program with std.
unsafe extern "C" {
    fn ape_hello(buf: *mut u8, n: i32) -> i32;
    fn ape_poll(port: i32) -> i32;
}

fn main() {
    let mut buf = [0u8; 64];
    let h = std::thread::spawn(|| std::fs::read_to_string("/dev/sysname").unwrap());
    let r = unsafe { ape_hello(buf.as_mut_ptr(), 64) };
    let s = h.join().unwrap();
    assert_eq!(r, 0);
    assert!(buf.starts_with(s.as_bytes()));
    // the C side's poll: a Rust thread serves it
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let srv = std::thread::spawn(move || {
        use std::io::{BufRead, Write};
        let (mut s, _) = l.accept().unwrap();
        let mut line = String::new();
        std::io::BufReader::new(&mut s).read_line(&mut line).unwrap();
        assert_eq!(line, "go\n");
        s.write_all(b"data\n").unwrap();
    });
    let r = unsafe { ape_poll(port as i32) };
    srv.join().unwrap();
    assert_eq!(r, 0, "ape_poll");
    println!("cmain: ok");
}
