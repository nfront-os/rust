//! C++ (libstdc++ threads over APE's pthreads) from Rust, also from a Rust thread.
unsafe extern "C" {
    fn cxx_test() -> i32;
}

fn main() {
    assert_eq!(unsafe { cxx_test() }, 0, "main");
    let r = std::thread::spawn(|| unsafe { cxx_test() }).join().unwrap();
    assert_eq!(r, 0, "thread");
    println!("cxxmain: ok");
}
