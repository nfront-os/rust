//! C's exit and abort in a Rust program with another thread running: the program ends whole.
unsafe extern "C" {
    fn c_exit(how: i32) -> !;
}

fn main() {
    let how: i32 = std::env::args().nth(1).map_or(0, |a| a.parse().unwrap());
    std::thread::spawn(|| loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    });
    std::thread::sleep(std::time::Duration::from_millis(100));
    unsafe { c_exit(how) }
}
