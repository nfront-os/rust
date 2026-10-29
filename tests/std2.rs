//! std on 9front: files, directories, the environment, processes, the network.
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::os::plan9::prelude::*;
use std::process::{Command, Stdio};
use std::{env, fs, thread};

fn check(name: &str, ok: bool) {
    if !ok {
        eprintln!("std2: FAIL {name}");
        std::process::exit(1);
    }
    println!("std2: {name} ok");
}

fn files() {
    let d = env::temp_dir().join(format!("rusttest.{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(d.join("a/b")).unwrap();
    let f = d.join("a/f.txt");
    fs::write(&f, b"hello\n").unwrap();
    check("read", fs::read_to_string(&f).unwrap() == "hello\n");
    let mut o = fs::OpenOptions::new().append(true).open(&f).unwrap();
    o.write_all(b"world\n").unwrap();
    drop(o);
    check("append", fs::read_to_string(&f).unwrap() == "hello\nworld\n");
    check("create_new", fs::OpenOptions::new().write(true).create_new(true).open(&f).unwrap_err().kind() == std::io::ErrorKind::AlreadyExists);
    let mut h = fs::OpenOptions::new().read(true).write(true).create(true).open(&f).unwrap();
    check("create keeps", h.metadata().unwrap().len() == 12);
    h.seek(SeekFrom::Start(6)).unwrap();
    let mut s = String::new();
    h.read_to_string(&mut s).unwrap();
    check("seek", s == "world\n");
    check("read_at", { let mut b = [0u8; 5]; h.read_at(&mut b, 0).unwrap() == 5 && &b == b"hello" });
    h.set_len(5).unwrap();
    check("set_len", fs::metadata(&f).unwrap().len() == 5);
    let m = fs::metadata(&f).unwrap();
    check("metadata", m.is_file() && !m.is_dir() && m.modified().is_ok() && !m.uid().is_empty());
    let mut p = m.permissions();
    p.set_readonly(true);
    fs::set_permissions(&f, p).unwrap();
    check("readonly", fs::metadata(&f).unwrap().permissions().readonly());
    let mut p = fs::metadata(&f).unwrap().permissions();
    p.set_mode(0o644);
    fs::set_permissions(&f, p).unwrap();
    check("mode", fs::metadata(&f).unwrap().permissions().mode() & 0o777 == 0o644);
    fs::rename(&f, d.join("a/g.txt")).unwrap();
    check("rename", !f.exists() && d.join("a/g.txt").exists());
    fs::write(d.join("a/h.txt"), b"x").unwrap();
    fs::rename(d.join("a/g.txt"), d.join("a/h.txt")).unwrap();
    check("rename replaces", fs::read_to_string(d.join("a/h.txt")).unwrap() == "hello");
    fs::rename(d.join("a/h.txt"), d.join("a/b/h.txt")).unwrap();
    check("rename across", fs::read_to_string(d.join("a/b/h.txt")).unwrap() == "hello");
    let mut names: Vec<_> = fs::read_dir(d.join("a")).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    names.sort();
    check("read_dir", names == ["b"]);
    check("remove_file dir", fs::remove_file(d.join("a/b")).is_err());
    check("copy", fs::copy(d.join("a/b/h.txt"), d.join("c.txt")).unwrap() == 5);
    check("canonicalize", fs::canonicalize(d.join("a/../c.txt")).unwrap() == d.join("c.txt"));
    check("not found", fs::metadata(d.join("nope")).unwrap_err().kind() == std::io::ErrorKind::NotFound);
    fs::remove_dir_all(&d).unwrap();
    check("remove_dir_all", !d.exists());
}

fn environment() {
    unsafe { env::set_var("rusttest", "v1") };
    check("env", env::var("rusttest").unwrap() == "v1" && env::vars().any(|(k, v)| k == "rusttest" && v == "v1"));
    unsafe { env::remove_var("rusttest") };
    check("env remove", env::var_os("rusttest").is_none());
    let path: Vec<_> = env::split_paths(&env::var_os("path").unwrap()).collect();
    check("path", path.iter().any(|p| p == std::path::Path::new("/bin")));
    let cwd = env::current_dir().unwrap();
    env::set_current_dir("/tmp").unwrap();
    check("chdir", env::current_dir().unwrap() == std::path::Path::new("/tmp"));
    env::set_current_dir(cwd).unwrap();
    check("current_exe", env::current_exe().unwrap().ends_with("std2"));
}

fn processes() {
    let o = Command::new("echo").args(["a", "b c"]).output().unwrap();
    check("output", o.status.success() && o.stdout == b"a b c\n");
    let o = Command::new("rc").args(["-c", "echo $x; pwd; exit 3"]).env("x", "y").current_dir("/tmp").output().unwrap();
    check("env cwd", o.stdout == b"y\n/tmp\n");
    check("status", !o.status.success() && o.status.code() == Some(3) && o.status.msg().ends_with(b"3"));
    let mut c = Command::new("cat").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    c.stdin.take().unwrap().write_all(b"piped\n").unwrap();
    let mut s = String::new();
    c.stdout.take().unwrap().read_to_string(&mut s).unwrap();
    check("pipes", s == "piped\n" && c.wait().unwrap().success());
    let mut c = Command::new("sleep").arg("10").spawn().unwrap();
    let t = std::time::Instant::now();
    c.kill().unwrap();
    let st = c.wait().unwrap();
    check("kill", !st.success() && t.elapsed().as_secs() < 5);
    let c = Command::new("rc").args(["-c", "exit oops"]).spawn().unwrap();
    let st = thread::spawn(move || { let mut c = c; c.wait().unwrap() }).join().unwrap();
    check("wait in a thread", st.msg().ends_with(b"oops") && st.code().is_none());
    check("not found", Command::new("/bin/nonexistent").spawn().unwrap_err().kind() == std::io::ErrorKind::NotFound);
    let o = Command::new("rc").args(["-c", "echo $#rusttestvar"]).env_clear().output().unwrap();
    check("env_clear", o.stdout == b"0\n");
}

fn network() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    check("bind", addr.port() != 0);
    let h = thread::spawn(move || {
        let (mut s, peer) = l.accept().unwrap();
        let mut line = String::new();
        BufReader::new(&mut s).read_line(&mut line).unwrap();
        s.write_all(line.to_uppercase().as_bytes()).unwrap();
        (peer, s)
    });
    let mut c = TcpStream::connect(addr).unwrap();
    c.write_all(b"ping\n").unwrap();
    let mut line = String::new();
    BufReader::new(&mut c).read_line(&mut line).unwrap();
    let (peer, _server) = h.join().unwrap();
    check("tcp", line == "PING\n" && peer == c.local_addr().unwrap() && c.peer_addr().unwrap() == addr);
    c.set_read_timeout(Some(std::time::Duration::from_millis(200))).unwrap();
    let e = c.read(&mut [0u8; 4]).unwrap_err();
    check("read timeout", e.kind() == std::io::ErrorKind::TimedOut);
    let a = UdpSocket::bind("127.0.0.1:0").unwrap();
    let b = UdpSocket::bind("127.0.0.1:0").unwrap();
    a.send_to(b"dgram", b.local_addr().unwrap()).unwrap();
    let mut buf = [0u8; 16];
    let (n, from) = b.recv_from(&mut buf).unwrap();
    check("udp", &buf[..n] == b"dgram" && from == a.local_addr().unwrap());
    let r: Vec<_> = std::net::ToSocketAddrs::to_socket_addrs("localhost:80").map(|i| i.collect()).unwrap_or_default();
    println!("std2: localhost -> {r:?}");
}

fn main() {
    let what = env::args().nth(1).unwrap_or_default();
    if what.is_empty() || what == "files" { files() }
    if what.is_empty() || what == "env" { environment() }
    if what.is_empty() || what == "proc" { processes() }
    if what.is_empty() || what == "net" { network() }
    println!("std2: all ok");
}
