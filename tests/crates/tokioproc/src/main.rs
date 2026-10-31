// tokio::process and tokio::signal on 9front. Run in a note group of its own (rc: rfork s; exec it):
// it posts notes to its group.
// Processes: output, an exit status, 1 MiB through cat, kill, kill_on_drop, try_wait,
// 10 children at once, a program that does not exist. Notes: ctrl_c (interrupt to the
// group: every proc gets it, the program goes on), plan9::note, std's count (one note to
// the group counts once), and a child's pipe read while the group's note interrupts it.
use std::io::ErrorKind;
use std::os::plan9::note::catch;
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

fn check(name: &str, ok: bool) {
    if ok {
        println!("tokioproc: {name} ok");
    } else {
        println!("tokioproc: FAIL {name}");
        std::process::exit(1);
    }
}

/// Posts `note` to this program's note group: every proc of it but the one that posts
/// (the kernel's postnotepg), hence from a thread of its own, as the console's Delete
/// reaches main too.
fn notepg(note: &str) {
    let path = format!("/proc/{}/notepg", std::process::id());
    let note = note.to_string();
    std::thread::spawn(move || std::fs::write(path, note).unwrap()).join().unwrap();
}

fn alive(pid: u32) -> bool {
    std::fs::metadata(format!("/proc/{pid}/status")).is_ok()
}

#[tokio::main]
async fn main() {
    // processes
    let o = Command::new("/bin/echo").arg("hello").output().await.unwrap();
    check("output", o.status.success() && o.stdout == b"hello\n");
    let s = Command::new("/bin/rc").args(["-c", "exit oops"]).status().await.unwrap();
    println!("tokioproc: exit status {s:?}");
    check("exit status", !s.success());

    let mut c = Command::new("/bin/cat").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    let mut stdin = c.stdin.take().unwrap();
    let mut stdout = c.stdout.take().unwrap();
    let data: Vec<u8> = (0..1 << 20).map(|i: usize| (i * 7 % 251) as u8).collect();
    let d = data.clone();
    let w = tokio::spawn(async move {
        stdin.write_all(&d).await.unwrap();
        stdin.shutdown().await.unwrap();
    });
    let mut back = Vec::new();
    stdout.read_to_end(&mut back).await.unwrap();
    w.await.unwrap();
    check("cat 1 MiB", back == data && c.wait().await.unwrap().success());

    let mut c = Command::new("/bin/sleep").arg("100").spawn().unwrap();
    check("try_wait running", c.try_wait().unwrap().is_none());
    let t = Instant::now();
    c.kill().await.unwrap();
    let s = c.wait().await.unwrap();
    check("kill", !s.success() && t.elapsed() < Duration::from_secs(5));
    check("try_wait done", c.try_wait().unwrap() == Some(s));

    let c = Command::new("/bin/sleep").arg("100").kill_on_drop(true).spawn().unwrap();
    let pid = c.id().unwrap();
    drop(c);
    let t = Instant::now();
    while alive(pid) && t.elapsed() < Duration::from_secs(5) {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    check("kill_on_drop", !alive(pid));

    let v: Vec<_> = (0..10)
        .map(|i| tokio::spawn(Command::new("/bin/echo").arg(format!("n{i}")).output()))
        .collect();
    let mut ok = true;
    for (i, t) in v.into_iter().enumerate() {
        let o = t.await.unwrap().unwrap();
        ok &= o.status.success() && o.stdout == format!("n{i}\n").as_bytes();
    }
    check("10 at once", ok);
    let e = Command::new("/bin/no-such-program").spawn();
    check("not found", matches!(e, Err(e) if e.kind() == ErrorKind::NotFound));

    // notes: std's count, once for the group
    let h = catch("rust-test hangup").unwrap();
    notepg("rust-test hangup");
    check("std note", h.wait() == 1);
    std::thread::sleep(Duration::from_millis(300));
    check("std note once", h.take() == 0);
    check("uncatchable", catch("sys: trap").is_err() && catch("alarm").is_err());

    // ctrl_c: interrupt to the group; a child (its own group) writes to a pipe read meanwhile
    let mut late = Command::new("/bin/rc")
        .args(["-c", "rfork s; sleep 1; echo late"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut out = late.stdout.take().unwrap();
    let read = tokio::spawn(async move {
        let mut s = String::new();
        out.read_to_string(&mut s).await.map(|_| s)
    });
    let cc = tokio::spawn(tokio::signal::ctrl_c());
    tokio::time::sleep(Duration::from_millis(300)).await;
    notepg("interrupt");
    let r = tokio::time::timeout(Duration::from_secs(5), cc).await;
    check("ctrl_c", matches!(r, Ok(Ok(Ok(())))));
    let s = read.await.unwrap();
    println!("tokioproc: pipe read across the note: {s:?}");
    check("pipe read across a note", matches!(&s, Ok(s) if s == "late\n"));
    check("child across a note", late.wait().await.unwrap().success());

    let mut n = tokio::signal::plan9::note("hangup").unwrap();
    let me = std::process::id();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        std::fs::write(format!("/proc/{me}/note"), "hangup").unwrap();
    });
    let r = tokio::time::timeout(Duration::from_secs(5), n.recv()).await;
    check("plan9::note", r == Ok(Some(())));
    println!("tokioproc: done");
}
