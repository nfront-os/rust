// SPDX-License-Identifier: MIT
//
// A window that outlives its wsys(4): when wsys is gone (killed, crashed;
// not when it ends in order, that posts hangup, nor when the window is
// deleted), the process waits for the user's next wsys and makes its window
// there, as wlsys does for Wayland's clients: mounted with the attach name
// new (where the window was), bound before /dev again, $wsys set to it. The
// readers then read the new window's files, draw(3) connects anew
// (draw::Draw::attach) and the program draws everything again (a resize).
// $wsys is /env/wsys (std's environment on Plan 9), which libdraw's
// wsysagain reads and writes too.

use crate::sys::{self, Fd, MBEFORE, MREPL, ORDWR};
use std::io;
use std::time::Duration;

/// the pid of the wsys whose /srv/wsys.user.pid $wsys names
fn pid(srv: &str) -> Option<u32> {
    srv.trim_end_matches('\0').rsplit('.').next()?.parse().ok()
}

fn alive(pid: u32) -> bool {
    std::fs::metadata(format!("/proc/{pid}")).is_ok()
}

/// the window's files are on a mount that hung up: its wsys is gone (its procs
/// may still be going: a dying wsys takes a while on a GPU), not just the
/// window (then wsys answers otherwise)
fn hungup() -> bool {
    match Fd::open("/mnt/wsys/winname", crate::sys::OREAD) {
        Ok(_) => false,
        Err(e) => e.to_string().contains("hungup"),
    }
}

/// the window's wsys is gone: under a wsys ($wsys: /srv/wsys.user.pid; rio's
/// windows go with it, as they did), whose process is no more or whose mount
/// hung up. Asked when a read of the window's files failed
pub fn lost() -> bool {
    let Ok(srv) = std::env::var("wsys") else { return false };
    let srv = srv.trim_end_matches('\0');
    if !srv.starts_with("/srv/wsys.") {
        return false;
    }
    match pid(srv) {
        Some(p) => !alive(p) || hungup(),
        None => false,
    }
}

/// the user's newest wsys still running: its /srv entry
fn newest() -> Option<String> {
    let user = std::env::var("user").unwrap_or_else(|_| "none".into());
    let prefix = format!("wsys.{user}.");
    std::fs::read_dir("/srv")
        .ok()?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let p: u32 = name.strip_prefix(&prefix)?.parse().ok()?;
            alive(p).then(|| (p, format!("/srv/{name}")))
        })
        .max()
        .map(|(_, path)| path)
}

/// $wsys names a wsys that runs: the window is there (made again by another:
/// Mesa's WSI in the same program, libdraw's wsysagain)
fn current_alive() -> bool {
    // not only its process: a killed wsys' procs stay a while after its mount hung up
    std::env::var("wsys").ok().as_deref().and_then(pid).is_some_and(alive) && !hungup()
}

/// one at a time in the program
static AGAIN: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// the window again in the next wsys, at rect (x0 y0 x1 y1, its place in the
/// old one) if known; waits for that wsys, a look a second, $wsyswait seconds
/// (0: no end; 5 minutes without it), then an error. If another made it
/// meanwhile ($wsys runs again), nothing more
pub fn reattach(rect: Option<&str>) -> io::Result<()> {
    let _one = AGAIN.lock().unwrap_or_else(|e| e.into_inner());
    let spec = match rect {
        Some(r) => format!("new -r {r} -pid {}", std::process::id()),
        None => format!("new -pid {}", std::process::id()),
    };
    // $wsyswait seconds (0: no end), else 5 minutes; then the program ends, as under rio
    let secs: u64 = std::env::var("wsyswait").ok().and_then(|v| v.trim_end_matches('\0').trim().parse().ok()).unwrap_or(300);
    let mut waited = 0;
    while secs == 0 || waited < secs {
        waited += 1;
        if current_alive() {
            return Ok(());
        }
        if let Some(path) = newest() {
            if let Ok(fd) = Fd::open(&path, ORDWR) {
                // the dead one's files out of /dev (bound before it: wsys(4)), the new window's in
                let _ = sys::unmount("/mnt/wsys", "/dev");
                if sys::mount(fd, "/mnt/wsys", MREPL, &spec).is_ok() {
                    sys::bind("/mnt/wsys", "/dev", MBEFORE)?;
                    // before the readers start again: no other thread of ours reads $wsys meanwhile
                    std::env::set_var("wsys", &path);
                    return Ok(());
                }
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    Err(io::Error::other("no next wsys in time ($wsyswait)"))
}
