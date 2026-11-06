// SPDX-License-Identifier: MIT
//
// what winittest and eguitest share (each uses some): their report, the window's pixels
// (/dev/window: image(6), as tests/sdl/sdltest.c reads them), its cursor
// (/dev/cursor) and title (/dev/label)

#![allow(dead_code)]

use std::io::Write;

#[derive(Default)]
pub struct Report {
    pub nfail: usize,
}

impl Report {
    pub fn ok(&mut self, s: &str) {
        println!("ok {s}");
        let _ = std::io::stdout().flush();
    }

    pub fn fail(&mut self, s: &str) {
        println!("FAIL {s}");
        let _ = std::io::stdout().flush();
        self.nfail += 1;
    }

    /// the summary line; the exit status
    pub fn summary(&self, name: &str) -> i32 {
        if self.nfail > 0 {
            println!("{name}: {} failed", self.nfail);
            1
        } else {
            println!("{name}: all passed");
            0
        }
    }
}

/// the window's pixel at x y inside its border of 4 (0xRRGGBB), from /dev/window
pub fn pixel(x: i32, y: i32) -> Result<u32, String> {
    let b = std::fs::read("/dev/window").map_err(|e| format!("/dev/window: {e}"))?;
    if b.len() < 60 {
        return Err("/dev/window: short".into());
    }
    let chan = String::from_utf8_lossy(&b[..12]).trim().to_string();
    if chan != "x8r8g8b8" && chan != "a8r8g8b8" {
        return Err(format!("/dev/window: channels {chan}"));
    }
    let f = |i: usize| String::from_utf8_lossy(&b[12 * i..12 * i + 12]).trim().parse::<i64>().unwrap_or(0);
    let (minx, miny, maxx) = (f(1), f(2), f(3));
    let (x, y) = (x as i64 + minx + 4, y as i64 + miny + 4);
    let off = (60 + ((y - miny) * (maxx - minx) + (x - minx)) * 4) as usize;
    if off + 4 > b.len() {
        return Err(format!("/dev/window: {x},{y} outside"));
    }
    Ok(b[off] as u32 | (b[off + 1] as u32) << 8 | (b[off + 2] as u32) << 16)
}

pub fn check(r: &mut Report, what: &str, x: i32, y: i32, want: u32) {
    match pixel(x, y) {
        Ok(got) if got == want => r.ok(&format!("{what}: at {x},{y} {got:06x}")),
        Ok(got) => r.fail(&format!("{what}: at {x},{y} {got:06x}, want {want:06x}")),
        Err(e) => r.fail(&format!("{what}: {e}")),
    }
}

/// the window's cursor as mouse(3) gives it: 72 bytes, none for the default;
/// None where it cannot be read (rio)
pub fn readcursor() -> Option<Vec<u8>> {
    std::fs::read("/dev/cursor").ok()
}

pub fn label() -> String {
    std::fs::read_to_string("/dev/label").unwrap_or_default()
}
