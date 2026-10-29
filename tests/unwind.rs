//! panic=unwind on 9front: a thread's panic is its join's error, catch_unwind, drops
//! while unwinding, a backtrace.
use std::panic;
use std::sync::atomic::{AtomicUsize, Ordering};

static DROPS: AtomicUsize = AtomicUsize::new(0);
struct D;
impl Drop for D {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::SeqCst);
    }
}

#[inline(never)]
fn deep(n: u32) -> u32 {
    let _d = D;
    if n == 0 { panic!("deep panic") }
    deep(n - 1) + 1
}

fn main() {
    let r = std::thread::spawn(|| deep(10)).join();
    let msg = r.unwrap_err().downcast::<&str>().map(|s| *s).unwrap_or("?");
    assert_eq!(msg, "deep panic");
    assert_eq!(DROPS.load(Ordering::SeqCst), 11);
    let r = panic::catch_unwind(|| deep(3));
    assert!(r.is_err());
    assert_eq!(DROPS.load(Ordering::SeqCst), 15);
    let bt = std::backtrace::Backtrace::force_capture();
    println!("unwind: backtrace {:?}", bt.status());
    println!("{bt}");
    println!("unwind: ok");
    if std::env::args().nth(1).as_deref() == Some("main") {
        deep(1);
    }
}
