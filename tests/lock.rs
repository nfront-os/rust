// std::os::plan9::fs::lock_file: lock DIR [nolock]. DIR on a file server that keeps DMEXCL
// (ramfs): the lock is exclusive, a waiter gets it once it is dropped, a plain file becomes
// exclusive-use. "any": as the server has it (ext4srv keeps no DMEXCL: every lock is had at
// once, nothing hangs).
use std::io::{ErrorKind, Write};
use std::os::plan9::fs::{MetadataExt, lock_file};
use std::time::{Duration, Instant};
use std::{env, fs, thread};

const DMEXCL: u32 = 0x2000_0000;

fn main() {
    let a: Vec<String> = env::args().collect();
    let dir = &a[1];
    let any = a.get(2).map(|s| s.as_str()) == Some("any");
    let p = format!("{dir}/l");
    let _ = fs::remove_file(&p);

    let l = lock_file(&p, false).expect("lock");
    let m = fs::metadata(&p).unwrap().mode();
    let keeps = m & DMEXCL != 0;
    assert!(keeps || any, "mode {m:#o}");
    println!("lock: {dir} keeps DMEXCL: {keeps}");
    let second = lock_file(&p, false);
    if keeps {
        assert_eq!(second.err().map(|e| e.kind()), Some(ErrorKind::WouldBlock));
        // a waiter gets it once the lock is dropped
        let p2 = p.clone();
        let t0 = Instant::now();
        let w = thread::spawn(move || {
            let _l = lock_file(&p2, true).expect("wait");
            t0.elapsed()
        });
        thread::sleep(Duration::from_millis(500));
        let d0 = Instant::now();
        drop(l);
        // the keep-alive thread ends at once
        assert!(d0.elapsed() < Duration::from_secs(2), "drop {:?}", d0.elapsed());
        let waited = w.join().unwrap();
        assert!(waited >= Duration::from_millis(500), "waited {waited:?}");
        println!("lock: waited {waited:?}");
    } else {
        second.expect("second lock without DMEXCL");
        drop(l);
    }

    // a file made before without DMEXCL
    let q = format!("{dir}/plain");
    let _ = fs::remove_file(&q);
    fs::File::create(&q).unwrap().write_all(b"x").unwrap();
    let l = lock_file(&q, true).expect("plain");
    assert_eq!(fs::metadata(&q).unwrap().mode() & DMEXCL != 0, keeps);
    if keeps {
        assert_eq!(lock_file(&q, false).err().map(|e| e.kind()), Some(ErrorKind::WouldBlock));
        assert_eq!(fs::File::open(&q).err().map(|e| e.kind()), Some(ErrorKind::WouldBlock));
    }
    let mut s = String::new();
    std::io::Read::read_to_string(&mut l.file(), &mut s).unwrap();
    assert_eq!(s, "x");
    drop(l);
    fs::File::open(&q).expect("open after unlock");
    println!("lock: done");
}
