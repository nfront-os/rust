//! 9front (Plan 9)
use crate::Error;
use core::mem::MaybeUninit;

pub use crate::util::{inner_u32, inner_u64};

unsafe extern "C" {
    fn open(file: *const core::ffi::c_char, mode: i32) -> i32;
    fn read(fd: i32, buf: *mut core::ffi::c_void, n: core::ffi::c_long) -> core::ffi::c_long;
    fn close(fd: i32) -> i32;
}

/// cons(3): `/dev/random`, the kernel's generator.
fn fill(dest: &mut [MaybeUninit<u8>]) -> Result<(), Error> {
    const OREAD_OCEXEC: i32 = 32;
    let fd = unsafe { open(b"/dev/random\0".as_ptr().cast(), OREAD_OCEXEC) };
    if fd < 0 {
        return Err(Error::UNEXPECTED);
    }
    let mut rest = dest;
    while !rest.is_empty() {
        let n = rest.len().min(32768);
        let r = unsafe { read(fd, rest.as_mut_ptr().cast(), n as core::ffi::c_long) };
        if r <= 0 {
            unsafe { close(fd) };
            return Err(Error::UNEXPECTED);
        }
        rest = &mut rest[r as usize..];
    }
    unsafe { close(fd) };
    Ok(())
}

#[inline]
pub fn fill_inner(dest: &mut [MaybeUninit<u8>]) -> Result<(), Error> {
    fill(dest)
}
