// SPDX-License-Identifier: MIT
//
// A window that outlives its wsys(4): when wsys is gone (killed, crashed;
// not when it ends in order, that posts hangup, nor when the window is
// deleted), the process waits for the user's next wsys and makes its window
// there, as wlsys does for Wayland's clients: mounted with the attach name
// new (where the window was), bound before /dev again, $wsys set to it. The
// readers then read the new window's files, draw(3) connects anew
// (draw::Draw::attach) and the program draws everything again (a resize).

use crate::sys::{self, Fd, MBEFORE, MREPL, ORDWR};
use std::io;
use std::time::Duration;

/// the pid of the wsys whose /srv/wsys.user.pid $wsys names
fn pid(srv: &str) -> Option<u32> {
    srv.rsplit('.').next()?.parse().ok()
}

fn alive(pid: u32) -> bool {
    std::fs::metadata(format!("/proc/{pid}")).is_ok()
}

/// the window's wsys is gone: under a wsys ($wsys), whose process is no more.
/// Asked when a read of the window's files failed, which happens while a
/// killed wsys' procs are still going: up to 3 s for it to be gone (a window
/// deleted, its wsys still there, is told that much later)
pub fn lost() -> bool {
    let Some(p) = std::env::var("wsys").ok().as_deref().and_then(pid) else { return false };
    for _ in 0..30 {
        if !alive(p) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
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

/// the window again in the next wsys, at rect (x0 y0 x1 y1, its place in the
/// old one) if known; waits for that wsys, a look a second
pub fn reattach(rect: Option<&str>) -> io::Result<()> {
    let spec = match rect {
        Some(r) => format!("new -r {r} -pid {}", std::process::id()),
        None => format!("new -pid {}", std::process::id()),
    };
    loop {
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
}
