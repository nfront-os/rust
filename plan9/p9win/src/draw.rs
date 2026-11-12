// SPDX-License-Identifier: MIT
//
// A connection of the program's own to draw(3), as libdraw has one (its
// messages as libdraw's init.c writes them, little-endian; wsys' draw.rs is
// the fuller client): the process's window by its name (/dev/winname, as a
// rio client finds it after every resize: rio gives it a new name), or the
// screen without a window system; pixels loaded into it.

use crate::sys::{Fd, ORDWR, OREAD};
use std::io;

/// x8r8g8b8: what the window's pixels are on 9front's screens, and a u32 of
/// 0x00RRGGBB in memory (softbuffer's pixels)
pub const XRGB32: u32 = 0x6808_1828;
pub const ARGB32: u32 = 0x4808_1828;
const GREY1: u32 = 0x31;
/// rio's and wsys' border around the window image a client draws in
pub const BORDERWIDTH: i32 = 4;

/// n pixels at 96 dpi at dpi, rounded (libdraw's scalesize(2))
pub fn scalesize(dpi: i32, n: i32) -> i32 {
    if dpi <= 0 { n } else { (n * dpi + 48) / 96 }
}
// libdraw sends at most its buffer (8000 bytes without an iounit); the same here
const MAXMSG: usize = 8000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl Rect {
    pub fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Rect {
        Rect { x0, y0, x1, y1 }
    }
    pub fn dx(&self) -> i32 {
        self.x1 - self.x0
    }
    pub fn dy(&self) -> i32 {
        self.y1 - self.y0
    }
    pub fn empty(&self) -> bool {
        self.x0 >= self.x1 || self.y0 >= self.y1
    }
    pub fn clip(&self, o: &Rect) -> Rect {
        Rect::new(self.x0.max(o.x0), self.y0.max(o.y0), self.x1.min(o.x1), self.y1.min(o.y1))
    }
    pub fn add(&self, x: i32, y: i32) -> Rect {
        Rect::new(self.x0 + x, self.y0 + y, self.x1 + x, self.y1 + y)
    }
    pub fn inset(&self, n: i32) -> Rect {
        Rect::new(self.x0 + n, self.y0 + n, self.x1 - n, self.y1 - n)
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x0 && x < self.x1 && y >= self.y0 && y < self.y1
    }
}

/// a channel descriptor of draw(3) ("x8r8g8b8") as its number (strtochan(2))
fn strtochan(s: &str) -> Option<u32> {
    let b = s.as_bytes();
    if b.is_empty() || b.len() % 2 != 0 {
        return None;
    }
    let mut c = 0u32;
    for p in b.chunks(2) {
        let t = match p[0] {
            b'r' => 0,
            b'g' => 1,
            b'b' => 2,
            b'k' => 3,
            b'a' => 4,
            b'm' => 5,
            b'x' => 6,
            _ => return None,
        };
        let n = (p[1] as char).to_digit(10)?;
        c = c << 8 | (t << 4 | n);
    }
    Some(c)
}

/// the 12 fields of 12 characters devdraw answers with (draw(3): ctl)
fn info(b: &[u8]) -> io::Result<Vec<String>> {
    if b.len() < 144 {
        return Err(io::Error::other(format!("draw: short info {}", b.len())));
    }
    Ok(b[..144].chunks(12).map(|f| String::from_utf8_lossy(f).trim().to_string()).collect())
}

fn int(s: &str) -> i32 {
    s.parse().unwrap_or(0)
}

fn rect(m: &mut Vec<u8>, r: &Rect) {
    for v in [r.x0, r.y0, r.x1, r.y1] {
        m.extend_from_slice(&v.to_le_bytes());
    }
}

/// the process's window (or the screen) through a draw(3) connection
pub struct Draw {
    ctl: Fd,
    data: Fd,
    buf: Vec<u8>,
    next: u32,
    name: String,    // the window's, as /dev/winname said
    win: u32,        // its image id: 0 is the screen
    chan: u32,
    r: Rect,         // the window image's rectangle (with rio's border)
    border: i32,
    screen: Rect,
    opaque: u32,
    scratch: Option<(u32, i32, i32)>, // an x8r8g8b8 image to load into, where the window is not
}

impl Draw {
    pub fn open() -> io::Result<Draw> {
        let ctl = Fd::open("/dev/draw/new", ORDWR)?;
        let mut b = [0u8; 12 * 12 + 1];
        let n = ctl.read(&mut b)?;
        let f = info(&b[..n])?;
        let data = Fd::open(&format!("/dev/draw/{}/data", f[0]), ORDWR)?;
        let screen = Rect::new(int(&f[4]), int(&f[5]), int(&f[6]), int(&f[7]));
        let mut d = Draw {
            ctl,
            data,
            buf: Vec::with_capacity(MAXMSG),
            next: 16,
            name: String::new(),
            win: 0,
            chan: strtochan(&f[2]).unwrap_or(XRGB32),
            r: screen,
            border: 0,
            screen,
            opaque: 0,
            scratch: None,
        };
        d.opaque = d.alloc(Rect::new(0, 0, 1, 1), GREY1, true, 0xFFFF_FFFF)?;
        d.attach()?;
        Ok(d)
    }

    fn id(&mut self) -> u32 {
        self.next += 1;
        self.next
    }

    fn cmd(&mut self, m: &[u8]) -> io::Result<()> {
        if self.buf.len() + m.len() > MAXMSG {
            self.flushbuf()?;
        }
        if m.len() > MAXMSG {
            return self.data.write1(m);
        }
        self.buf.extend_from_slice(m);
        Ok(())
    }

    fn flushbuf(&mut self) -> io::Result<()> {
        if !self.buf.is_empty() {
            let r = self.data.write1(&self.buf);
            self.buf.clear();
            r?;
        }
        Ok(())
    }

    /// 'v': what is drawn reaches the screen
    pub fn flush(&mut self) -> io::Result<()> {
        self.buf.push(b'v');
        self.flushbuf()
    }

    /// 'b': a new image; repl images of 1x1 are colours
    fn alloc(&mut self, r: Rect, chan: u32, repl: bool, col: u32) -> io::Result<u32> {
        let id = self.id();
        let mut m = vec![b'b'];
        m.extend_from_slice(&id.to_le_bytes());
        m.extend_from_slice(&0u32.to_le_bytes()); // screen id: none
        m.push(0); // refresh
        m.extend_from_slice(&chan.to_le_bytes());
        m.push(repl as u8);
        rect(&mut m, &r);
        let clip = if repl { Rect::new(-0x3FFF_FFFF, -0x3FFF_FFFF, 0x3FFF_FFFF, 0x3FFF_FFFF) } else { r };
        rect(&mut m, &clip);
        m.extend_from_slice(&col.to_le_bytes());
        self.cmd(&m)?;
        Ok(id)
    }

    fn free(&mut self, id: u32) -> io::Result<()> {
        let mut m = vec![b'f'];
        m.extend_from_slice(&id.to_le_bytes());
        self.cmd(&m)
    }

    /// the window again, by the name /dev/winname has now (rio names a window
    /// anew when it changes); without /dev/winname the screen
    pub fn attach(&mut self) -> io::Result<()> {
        // the connection's devdraw gone (the window's wsys: wsys.rs made the
        // window again in the next one): a new connection, its images anew
        // (a flush: harmless, it fails only there; ctl cannot tell, a new connection's
        // ctl is read once)
        if self.flush().is_err() {
            *self = Draw::open()?;
            return Ok(());
        }
        let name = match Fd::open("/dev/winname", OREAD) {
            Ok(fd) => {
                let mut b = [0u8; 256];
                let n = fd.read(&mut b)?;
                String::from_utf8_lossy(&b[..n]).trim().to_string()
            }
            Err(_) => String::new(),
        };
        if name.is_empty() {
            if self.win != 0 {
                let w = self.win;
                self.free(w)?;
            }
            self.win = 0;
            self.name.clear();
            self.r = self.screen;
            self.border = 0;
            return Ok(());
        }
        if self.win != 0 && name == self.name {
            return Ok(());
        }
        if self.win != 0 {
            let w = self.win;
            self.free(w)?;
            self.win = 0;
        }
        self.flushbuf()?;
        let id = self.id();
        let mut m = vec![b'n'];
        m.extend_from_slice(&id.to_le_bytes());
        m.push(name.len() as u8);
        m.extend_from_slice(name.as_bytes());
        self.cmd(&m)?;
        self.flushbuf()?;
        let mut b = [0u8; 12 * 12 + 1];
        let n = self.ctl.pread(&mut b, 0)?;
        let f = info(&b[..n])?;
        self.win = id;
        self.chan = strtochan(&f[2]).unwrap_or(XRGB32);
        self.r = Rect::new(int(&f[4]), int(&f[5]), int(&f[6]), int(&f[7]));
        // rio's windows have a border; devdraw's screen (noborder.screen.N) has none;
        // at another scale than 1 (wsys' dpi) libdraw's scalesize(BORDERWIDTH)
        self.border = if name.starts_with("noborder.") { 0 } else { scalesize(crate::dpi(), BORDERWIDTH) };
        self.name = name;
        Ok(())
    }

    /// the window's contents on the screen: inside the border
    pub fn content(&self) -> Rect {
        self.r.inset(self.border)
    }

    /// the border a window system draws around the contents
    pub fn border(&self) -> i32 {
        self.border
    }

    /// the whole screen
    pub fn screen(&self) -> Rect {
        self.screen
    }

    /// w x h pixels of 0x00RRGGBB at the window's top left, those in the
    /// rectangles of damage (all of them if there are none)
    pub fn put(&mut self, px: &[u32], w: usize, h: usize, damage: &[Rect]) -> io::Result<()> {
        let c = self.content();
        let all = [Rect::new(0, 0, w as i32, h as i32)];
        let damage = if damage.is_empty() { &all[..] } else { damage };
        let bytes = unsafe { std::slice::from_raw_parts(px.as_ptr() as *const u8, px.len() * 4) };
        let stride = w * 4;
        let direct = self.chan == XRGB32 || self.chan == ARGB32;
        let target = if direct {
            self.win
        } else {
            // a window of other pixels: into an image of these, drawn into it
            if !matches!(self.scratch, Some((_, sw, sh)) if sw == w as i32 && sh == h as i32) {
                if let Some((id, _, _)) = self.scratch.take() {
                    self.free(id)?;
                }
                let id = self.alloc(Rect::new(0, 0, w as i32, h as i32), XRGB32, false, 0)?;
                self.scratch = Some((id, w as i32, h as i32));
            }
            self.scratch.unwrap().0
        };
        for d in damage {
            // in the buffer, then on the screen (inside the window)
            let d = d.clip(&all[0]).clip(&Rect::new(0, 0, c.dx(), c.dy()));
            if d.empty() {
                continue;
            }
            if direct {
                self.load_at(target, &d.add(c.x0, c.y0), bytes, stride, d.x0, d.y0)?;
            } else {
                self.load_at(target, &d, bytes, stride, d.x0, d.y0)?;
                let (win, opaque) = (self.win, self.opaque);
                let mut m = vec![b'd'];
                for id in [win, target, opaque] {
                    m.extend_from_slice(&id.to_le_bytes());
                }
                rect(&mut m, &d.add(c.x0, c.y0));
                for v in [d.x0, d.y0, 0, 0] {
                    m.extend_from_slice(&v.to_le_bytes());
                }
                self.cmd(&m)?;
            }
        }
        self.flush()
    }

    /// 'y': rectangle r of image id from pixels laid out with stride (x8r8g8b8),
    /// those for r's top left at (sx, sy), in messages of at most MAXMSG bytes:
    /// bands of rows, or pieces of a row too long for one
    fn load_at(&mut self, id: u32, r: &Rect, px: &[u8], stride: usize, sx: i32, sy: i32) -> io::Result<()> {
        let (dx, dy) = (sx - r.x0, sy - r.y0);
        let room = MAXMSG - 1 - 4 - 16;
        let bpl = r.dx() as usize * 4;
        let mut y = r.y0;
        while y < r.y1 {
            let (band, rows) = if bpl <= room {
                let rows = ((room / bpl) as i32).min(r.y1 - y);
                (Rect::new(r.x0, y, r.x1, y + rows), rows)
            } else {
                (Rect::new(r.x0, y, r.x1, y + 1), 1)
            };
            let mut x = band.x0;
            while x < band.x1 {
                let x1 = if bpl <= room { band.x1 } else { (x + (room / 4) as i32).min(band.x1) };
                let n = (x1 - x) as usize * 4;
                if self.buf.len() + 1 + 4 + 16 + n * rows as usize > MAXMSG {
                    self.flushbuf()?;
                }
                self.buf.push(b'y');
                self.buf.extend_from_slice(&id.to_le_bytes());
                rect(&mut self.buf, &Rect::new(x, band.y0, x1, band.y1));
                for yy in band.y0..band.y1 {
                    let o = (yy + dy) as usize * stride + (x + dx) as usize * 4;
                    self.buf.extend_from_slice(&px[o..o + n]);
                }
                x = x1;
            }
            y += rows;
        }
        Ok(())
    }
}
