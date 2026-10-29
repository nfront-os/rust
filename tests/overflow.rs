//! A stack overflow must fault (the kernel's "suicide: sys: trap: fault"), in main
//! (argument "main") or in a thread (argument "thread").
use std::hint::black_box;

fn deep(n: u64) -> u64 {
    let buf = black_box([n; 64]);
    if n == 0 { buf[0] } else { deep(n - 1) + black_box(buf[1]) }
}

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_default();
    if arg == "thread" {
        let h = std::thread::spawn(|| deep(u64::MAX));
        println!("overflow: joined {}", h.join().unwrap());
    } else {
        println!("overflow: {}", deep(u64::MAX));
    }
}
