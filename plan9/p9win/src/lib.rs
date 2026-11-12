// SPDX-License-Identifier: MIT
//
// p9win: the process's window of wsys or rio for Rust programs, the screen
// without either; what winit's and softbuffer's plan9 backends are made of.
// - draw: a draw(3) connection to the window (its place, pixels loaded into it)
// - input: readers of /dev/mouse, /dev/kbd and /dev/wctl, threads of their own
// - wsys: the window made again in the next wsys when its wsys is gone
// - cursor: /dev/cursor
// - the window system's other files: /dev/label, /dev/wctl, /dev/snarf

pub mod cursor;
pub mod draw;
pub mod sys;
pub mod wsys;

use std::io;
use std::sync::{Arc, Mutex};
use sys::{Fd, OREAD, ORDWR, OTRUNC, OWRITE};

/// Plan 9's runes for keys that are not characters (keyboard.h)
pub mod keys {
    pub const KF: u32 = 0xF000;
    pub const SPEC: u32 = 0xF800;
    pub const HOME: u32 = KF | 0x0D;
    pub const UP: u32 = KF | 0x0E;
    pub const DOWN: u32 = SPEC;
    pub const PGUP: u32 = KF | 0x0F;
    pub const PRINT: u32 = KF | 0x10;
    pub const LEFT: u32 = KF | 0x11;
    pub const RIGHT: u32 = KF | 0x12;
    pub const PGDOWN: u32 = KF | 0x13;
    pub const INS: u32 = KF | 0x14;
    pub const ALT: u32 = KF | 0x15;
    pub const SHIFT: u32 = KF | 0x16;
    pub const CTL: u32 = KF | 0x17;
    pub const END: u32 = KF | 0x18;
    pub const SCROLL: u32 = KF | 0x19;
    pub const BREAK: u32 = SPEC | 0x61;
    pub const CAPS: u32 = SPEC | 0x64;
    pub const NUM: u32 = SPEC | 0x65;
    pub const ALTGR: u32 = SPEC | 0x67;
    pub const MOD4: u32 = SPEC | 0x68;
    pub const BS: u32 = 0x08;
    pub const DEL: u32 = 0x7f;
    pub const ESC: u32 = 0x1b;
    /// F1..F12: KF|1 ..
    pub fn f(n: u32) -> u32 {
        KF | n
    }
}

/// what the readers read
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    /// the pointer at x y on the screen, buttons 1 2 4 (8 16: the wheel); resized:
    /// the window changed (look it up again)
    Mouse { x: i32, y: i32, buttons: u32, resized: bool },
    /// kbd(3)'s k and K: the keys down now (after one went down or up)
    Keys(Vec<char>),
    /// kbd(3)'s c: a character typed
    Char(char),
    /// a line of /dev/wctl: the window changed (place, current, hidden)
    Wctl(String),
    /// /dev/mouse ended: the window is gone
    Closed,
}

/// The window's input: readers of /dev/mouse, /dev/kbd and /dev/wctl, each a
/// thread, handing what they read to send; the mouse kept for warping. When
/// the window's wsys is gone the mouse's reader makes the window again in the
/// next one (wsys.rs) and says so as a resize; Closed only when the window is.
pub struct Inputs {
    mouse: Arc<Mutex<Arc<Fd>>>,
}

type Sender = Arc<dyn Fn(Input) + Send + Sync>;

/// readers of /dev/kbd and /dev/wctl (the window's now): until their file fails;
/// rect gets the window's place from wctl's lines (x0 y0 x1 y1 ...)
fn readers(send: &Sender, rect: &Arc<Mutex<Option<String>>>) -> io::Result<()> {
    if let Ok(kbd) = Fd::open("/dev/kbd", OREAD) {
        let send = send.clone();
        std::thread::Builder::new().name("p9win kbd".into()).spawn(move || {
            // messages k, K, c and a UTF-8 string each, NUL after each
            let mut b = [0u8; 512];
            while let Ok(n) = kbd.read(&mut b) {
                if n == 0 {
                    break;
                }
                for m in b[..n].split(|&c| c == 0).filter(|m| !m.is_empty()) {
                    let s = String::from_utf8_lossy(&m[1..]);
                    match m[0] {
                        b'k' | b'K' => send(Input::Keys(s.chars().collect())),
                        b'c' => {
                            if let Some(c) = s.chars().next() {
                                send(Input::Char(c))
                            }
                        }
                        _ => {}
                    }
                }
            }
        })?;
    }
    if let Ok(wctl) = Fd::open("/dev/wctl", OREAD) {
        let (send, rect) = (send.clone(), rect.clone());
        std::thread::Builder::new().name("p9win wctl".into()).spawn(move || {
            let mut b = [0u8; 256];
            while let Ok(n) = wctl.read(&mut b) {
                if n == 0 {
                    break;
                }
                let s = String::from_utf8_lossy(&b[..n]).into_owned();
                let f: Vec<&str> = s.split_whitespace().take(4).collect();
                if f.len() == 4 && f.iter().all(|x| x.parse::<i32>().is_ok()) {
                    *rect.lock().unwrap() = Some(f.join(" "));
                }
                send(Input::Wctl(s));
            }
        })?;
    }
    Ok(())
}

impl Inputs {
    pub fn start<F>(send: F) -> io::Result<Inputs>
    where
        F: Fn(Input) + Send + Sync + 'static,
    {
        let send: Sender = Arc::new(send);
        let rect = Arc::new(Mutex::new(None));
        let mouse = Arc::new(Mutex::new(Arc::new(Fd::open("/dev/mouse", ORDWR)?)));
        readers(&send, &rect)?;
        {
            let mouse = mouse.clone();
            std::thread::Builder::new().name("p9win mouse".into()).spawn(move || {
                loop {
                    let fd = mouse.lock().unwrap().clone();
                    // m x y buttons msec, 49 bytes a message; r: the window changed
                    let mut b = [0u8; 64];
                    while let Ok(n) = fd.read(&mut b[..49]) {
                        if n == 0 {
                            break;
                        }
                        let s = String::from_utf8_lossy(&b[..n]);
                        let k = s.as_bytes()[0];
                        if k != b'm' && k != b'r' {
                            continue;
                        }
                        let f: Vec<i64> = s[1..].split_whitespace().filter_map(|x| x.parse().ok()).collect();
                        if f.len() < 3 {
                            continue;
                        }
                        send(Input::Mouse { x: f[0] as i32, y: f[1] as i32, buttons: f[2] as u32, resized: k == b'r' });
                    }
                    // the window gone, or its wsys: then the window again in the next one
                    if !wsys::lost() {
                        break;
                    }
                    let r = rect.lock().unwrap().clone();
                    if wsys::reattach(r.as_deref()).is_err() {
                        break;
                    }
                    match Fd::open("/dev/mouse", ORDWR) {
                        Ok(f) => *mouse.lock().unwrap() = Arc::new(f),
                        Err(_) => break,
                    }
                    if readers(&send, &rect).is_err() {
                        break;
                    }
                    send(Input::Mouse { x: 0, y: 0, buttons: 0, resized: true });
                }
                send(Input::Closed);
            })?;
        }
        Ok(Inputs { mouse })
    }

    /// the pointer to x y on the screen
    pub fn warp(&self, x: i32, y: i32) -> io::Result<()> {
        let fd = self.mouse.lock().unwrap().clone();
        fd.write1(format!("m{x} {y}").as_bytes())
    }
}

/// a message to the window's /dev/wctl (rio(4): resize, move, hide, unhide,
/// current); without a window system nothing
/// the window system's dots per inch: its window's dpi file (wsys(4),
/// libdraw's display->dpi); 96 without one (rio)
pub fn dpi() -> i32 {
    std::fs::read_to_string("/dev/dpi").ok().and_then(|s| s.trim().parse().ok()).filter(|&d: &i32| d > 0).unwrap_or(96)
}

/// the scale the window system draws at: dpi/96 (1 under rio)
pub fn scale() -> f64 {
    dpi() as f64 / 96.0
}

pub fn wctl(msg: &str) -> io::Result<()> {
    match Fd::open("/dev/wctl", OWRITE) {
        Ok(fd) => fd.write1(msg.as_bytes()),
        Err(_) => Ok(()),
    }
}

/// the window's title: /dev/label
pub fn set_label(s: &str) -> io::Result<()> {
    Fd::open("/dev/label", OWRITE | OTRUNC)?.write1(s.as_bytes())
}

pub fn label() -> String {
    read_all("/dev/label").unwrap_or_default()
}

/// the snarf buffer, shared with rio's and wsys' windows
pub fn snarf() -> io::Result<String> {
    read_all("/dev/snarf")
}

pub fn set_snarf(s: &str) -> io::Result<()> {
    let fd = Fd::open("/dev/snarf", OWRITE | OTRUNC)?;
    for c in s.as_bytes().chunks(8192) {
        fd.write1(c)?;
    }
    Ok(())
}

fn read_all(name: &str) -> io::Result<String> {
    let fd = Fd::open(name, OREAD)?;
    let mut out = Vec::new();
    let mut b = [0u8; 8192];
    loop {
        let n = fd.read(&mut b)?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&b[..n]);
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}
