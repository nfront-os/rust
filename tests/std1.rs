//! std on 9front: output, arguments, threads (scoped: references to the stack of
//! another thread), locks, channels, thread-locals with destructors, time.
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Condvar, Mutex, RwLock, mpsc};
use std::time::{Duration, Instant, SystemTime};
use std::{env, thread};

static DROPS: AtomicUsize = AtomicUsize::new(0);

struct Counted;
impl Drop for Counted {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::SeqCst);
    }
}

thread_local! {
    static LOCAL: Cell<u32> = const { Cell::new(0) };
    static DTOR: Counted = Counted;
}

fn check(name: &str, ok: bool) {
    if !ok {
        eprintln!("std1: FAIL {name}");
        std::process::exit(1);
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    check("args", args.len() == 3 && args[1] == "a b" && args[2] == "ü");

    // scoped threads write into this thread's stack
    let mut v = [0u64; 8];
    thread::scope(|s| {
        for (i, x) in v.iter_mut().enumerate() {
            s.spawn(move || *x = (i as u64 + 1) * 10);
        }
    });
    check("scoped", v.iter().sum::<u64>() == 360);

    // a mutex under contention
    let m = Arc::new(Mutex::new(0u64));
    let hs: Vec<_> = (0..8)
        .map(|_| {
            let m = m.clone();
            thread::spawn(move || {
                for _ in 0..10000 {
                    *m.lock().unwrap() += 1;
                }
            })
        })
        .collect();
    for h in hs {
        h.join().unwrap();
    }
    check("mutex", *m.lock().unwrap() == 80000);

    // condvar and barrier
    let pair = Arc::new((Mutex::new(false), Condvar::new()));
    let p2 = pair.clone();
    let h = thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        *p2.0.lock().unwrap() = true;
        p2.1.notify_one();
    });
    let mut g = pair.0.lock().unwrap();
    while !*g {
        g = pair.1.wait(g).unwrap();
    }
    drop(g);
    h.join().unwrap();
    let (_g, to) = pair.1.wait_timeout(pair.0.lock().unwrap(), Duration::from_millis(30)).unwrap();
    check("condvar timeout", to.timed_out());
    let b = Arc::new(Barrier::new(4));
    let hs: Vec<_> = (0..4).map(|_| { let b = b.clone(); thread::spawn(move || { b.wait(); }) }).collect();
    hs.into_iter().for_each(|h| h.join().unwrap());
    let rw = RwLock::new(5);
    check("rwlock", *rw.read().unwrap() == 5);

    // channels: rendezvous (zero capacity, packets on the stack) and buffered
    let (tx, rx) = mpsc::sync_channel::<u32>(0);
    let h = thread::spawn(move || (0..100).for_each(|i| tx.send(i).unwrap()));
    check("sync_channel", rx.iter().sum::<u32>() == 4950);
    h.join().unwrap();
    let (tx, rx) = mpsc::channel();
    for i in 0..4 {
        let tx = tx.clone();
        thread::spawn(move || tx.send(i).unwrap());
    }
    drop(tx);
    check("channel", rx.iter().sum::<i32>() == 6);

    // thread-locals: per thread, destructors run when a thread ends
    LOCAL.with(|l| l.set(7));
    let h = thread::spawn(|| {
        DTOR.with(|_| {});
        LOCAL.with(|l| l.get())
    });
    check("tls", h.join().unwrap() == 0 && LOCAL.with(|l| l.get()) == 7);
    check("tls dtor", DROPS.load(Ordering::SeqCst) == 1);


    // time
    let t = Instant::now();
    thread::sleep(Duration::from_millis(100));
    let e = t.elapsed();
    check("sleep", e >= Duration::from_millis(100) && e < Duration::from_secs(5));
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
    check("systemtime", now.as_secs() > 1_700_000_000);
    check("parallelism", thread::available_parallelism().is_ok());
    check("float", format!("{:.3} {}", 1.0f64 / 3.0, 1.0f64 / 0.0) == "0.333 inf");
    let mut hm = std::collections::HashMap::new();
    hm.insert("k", 1);
    check("hashmap", hm["k"] == 1);
    println!("std1: ok");
    std::process::exit(if args.len() == 3 { 0 } else { 3 });
}
