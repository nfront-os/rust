//! core only, 9front's libc by hand: write, exits
#![no_std]
#![no_main]

use core::ffi::{c_char, c_int, c_long, c_void};

unsafe extern "C" {
    fn write(fd: c_int, buf: *const c_void, n: c_long) -> c_long;
    fn exits(msg: *const c_char) -> !;
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    unsafe { exits(c"panic".as_ptr()) }
}

#[unsafe(no_mangle)]
extern "C" fn main(argc: c_int, _argv: *const *const c_char) -> c_int {
    let msg = b"nostd: hello from Rust on 9front\n";
    unsafe { write(1, msg.as_ptr().cast(), msg.len() as c_long) };
    // 1.0/0.0 must not trap: __wrap_main masked the FPU exceptions
    let x = core::hint::black_box(1.0f64) / core::hint::black_box(0.0f64);
    if x != f64::INFINITY || argc < 1 {
        unsafe { exits(c"bad float".as_ptr()) }
    }
    unsafe { exits(core::ptr::null()) }
}
