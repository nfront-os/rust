// SPDX-License-Identifier: MIT
//
// The Plan 9 system calls p9win needs that std does not offer as they are,
// from 9front's own C library as Rust's std for plan9 links it (libp9.a: its
// names are __p9_*), as wsys has them (sys/src/cmd/wsys/src/p9.rs).

use std::ffi::{CString, c_char, c_int, c_long, c_uint, c_void};
use std::io;

pub const OREAD: c_int = 0;
pub const OWRITE: c_int = 1;
pub const ORDWR: c_int = 2;
pub const OTRUNC: c_int = 16;
/// close on exec: what the window's program opens stays out of what it starts
const OCEXEC: c_int = 32;

unsafe extern "C" {
    #[link_name = "__p9_open"]
    fn p9open(file: *const c_char, mode: c_int) -> c_int;
    #[link_name = "__p9_close"]
    fn p9close(fd: c_int) -> c_int;
    #[link_name = "__p9_read"]
    fn p9read(fd: c_int, buf: *mut c_void, n: c_long) -> c_long;
    #[link_name = "__p9_write"]
    fn p9write(fd: c_int, buf: *const c_void, n: c_long) -> c_long;
    #[link_name = "__p9_pread"]
    fn p9pread(fd: c_int, buf: *mut c_void, n: c_long, off: i64) -> c_long;
    #[link_name = "__p9_rerrstr"]
    fn p9rerrstr(buf: *mut c_char, n: c_uint);
    #[link_name = "__p9_pipe"]
    fn p9pipe(fd: *mut c_int) -> c_int;
    #[link_name = "__p9_create"]
    fn p9create(file: *const c_char, mode: c_int, perm: c_uint) -> c_int;
    #[link_name = "__p9_remove"]
    fn p9remove(file: *const c_char) -> c_int;
}

/// the process's error string as an io::Error
pub fn err() -> io::Error {
    let mut b = [0u8; 256];
    unsafe { p9rerrstr(b.as_mut_ptr() as *mut c_char, b.len() as c_uint) };
    let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    io::Error::other(String::from_utf8_lossy(&b[..n]).into_owned())
}

/// A file descriptor of Plan 9's: plain reads and writes, each one a system call
/// (draw(3) messages must not be split or joined; a write of nothing is one too:
/// /dev/cursor's default).
#[derive(Debug)]
pub struct Fd(pub c_int);

impl Fd {
    pub fn open(name: &str, mode: c_int) -> io::Result<Fd> {
        let name = CString::new(name).map_err(|_| io::Error::other("nul in name"))?;
        let fd = unsafe { p9open(name.as_ptr(), mode | OCEXEC) };
        if fd < 0 { Err(err()) } else { Ok(Fd(fd)) }
    }

    // an empty slice's pointer is dangling (0x1 for bytes), and Plan 9's kernel
    // checks the address even for no bytes ("bad address in syscall"): one of
    // its own then
    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let mut none = [0u8; 1];
        let p = if buf.is_empty() { none.as_mut_ptr() } else { buf.as_mut_ptr() };
        let n = unsafe { p9read(self.0, p as *mut c_void, buf.len() as c_long) };
        if n < 0 { Err(err()) } else { Ok(n as usize) }
    }

    /// a read at offset off (a ctl file's reply: from its start)
    pub fn pread(&self, buf: &mut [u8], off: i64) -> io::Result<usize> {
        let mut none = [0u8; 1];
        let p = if buf.is_empty() { none.as_mut_ptr() } else { buf.as_mut_ptr() };
        let n = unsafe { p9pread(self.0, p as *mut c_void, buf.len() as c_long, off) };
        if n < 0 { Err(err()) } else { Ok(n as usize) }
    }

    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        let none = [0u8; 1];
        let p = if buf.is_empty() { none.as_ptr() } else { buf.as_ptr() };
        let n = unsafe { p9write(self.0, p as *const c_void, buf.len() as c_long) };
        if n < 0 { Err(err()) } else { Ok(n as usize) }
    }

    /// all of buf in one write, or an error
    pub fn write1(&self, buf: &[u8]) -> io::Result<()> {
        let n = self.write(buf)?;
        if n != buf.len() {
            return Err(io::Error::other(format!("short write {n} of {}", buf.len())));
        }
        Ok(())
    }
}

impl Drop for Fd {
    fn drop(&mut self) {
        unsafe { p9close(self.0) };
    }
}

/// a pipe: its two ends (close on exec: through dup(3) again, pipe(2) has none)
pub fn pipe() -> io::Result<(Fd, Fd)> {
    let mut p = [0 as c_int; 2];
    if unsafe { p9pipe(p.as_mut_ptr()) } < 0 {
        return Err(err());
    }
    let (a, b) = (Fd(p[0]), Fd(p[1]));
    Ok((Fd::open(&format!("#d/{}", a.0), ORDWR)?, Fd::open(&format!("#d/{}", b.0), ORDWR)?))
}

/// fd posted as /srv/name (srv(3)) for others to mount; the entry goes when
/// the returned descriptor is closed (ORCLOSE): with the process, however it
/// ends (a window's program gets a note when the window is deleted)
pub fn post(name: &str, fd: &Fd) -> io::Result<Fd> {
    const ORCLOSE: c_int = 64;
    let path = format!("/srv/{name}");
    let cs = CString::new(path).map_err(|_| io::Error::other("nul in name"))?;
    let f = unsafe { p9create(cs.as_ptr(), OWRITE | ORCLOSE, 0o600) };
    if f < 0 {
        return Err(err());
    }
    let f = Fd(f);
    f.write1(fd.0.to_string().as_bytes())?;
    Ok(f)
}

pub fn remove(name: &str) -> io::Result<()> {
    let cs = CString::new(name).map_err(|_| io::Error::other("nul in name"))?;
    if unsafe { p9remove(cs.as_ptr()) } < 0 { Err(err()) } else { Ok(()) }
}
