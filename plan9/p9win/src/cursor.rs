// SPDX-License-Identifier: MIT
//
// Cursors of Plan 9: 16x16 pixels of two bits, clr (white) and set (black,
// over clr), and an offset from the pointer to the top left; /dev/cursor
// takes them as mouse(3)'s 72 bytes (rio and wsys keep one a window and show
// it while the pointer is in it; a write of nothing gives the arrow). The
// shapes drawn here are those of SDL's and GLFW's plan9 backends.

use crate::sys::{Fd, OWRITE};
use std::sync::Mutex;

/// mouse(3)'s cursor: offset x y (little endian), clr, set
pub type Cursor = [u8; 72];

/// a cursor of no pixels: none to be seen
pub const HIDDEN: Cursor = [0; 72];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Text,
    Wait,
    Crosshair,
    NwseResize,
    NeswResize,
    EwResize,
    NsResize,
    Move,
    NotAllowed,
    Pointer,
}

// # black, o white, . nothing; black gets a white edge
const ART: &[(Shape, i32, i32, [&str; 16])] = &[
    (Shape::Text, 7, 7, [
        "....###.###.....",
        ".......#........",
        ".......#........",
        ".......#........",
        ".......#........",
        ".......#........",
        ".......#........",
        ".......#........",
        ".......#........",
        ".......#........",
        ".......#........",
        ".......#........",
        ".......#........",
        "....###.###.....",
        "................",
        "................",
    ]),
    (Shape::Wait, 7, 7, [
        "..###########...",
        "..#.........#...",
        "...#.......#....",
        "....#.....#.....",
        ".....#...#......",
        "......#.#.......",
        ".......#........",
        "......#.#.......",
        ".....#...#......",
        "....#..#..#.....",
        "...#..###..#....",
        "..#..#####..#...",
        "..###########...",
        "................",
        "................",
        "................",
    ]),
    (Shape::Crosshair, 7, 7, [
        ".......#........",
        ".......#........",
        ".......#........",
        ".......#........",
        ".......#........",
        "................",
        "................",
        "#####.....#####.",
        "................",
        "................",
        ".......#........",
        ".......#........",
        ".......#........",
        ".......#........",
        ".......#........",
        "................",
    ]),
    (Shape::NwseResize, 7, 7, [
        "................",
        ".#####..........",
        ".##.............",
        ".#.#............",
        ".#..#...........",
        ".#...#..........",
        "......#.........",
        ".......#........",
        "........#.......",
        ".........#...#..",
        "..........#..#..",
        "...........#.#..",
        "............##..",
        ".........#####..",
        "................",
        "................",
    ]),
    (Shape::NeswResize, 7, 7, [
        "................",
        ".........#####..",
        "............##..",
        "...........#.#..",
        "..........#..#..",
        ".........#...#..",
        "........#.......",
        ".......#........",
        "......#.........",
        ".#...#..........",
        ".#..#...........",
        ".#.#............",
        ".##.............",
        ".#####..........",
        "................",
        "................",
    ]),
    (Shape::EwResize, 7, 7, [
        "................",
        "................",
        "................",
        "................",
        "................",
        "..#.........#...",
        ".##.........##..",
        "###############.",
        ".##.........##..",
        "..#.........#...",
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
    ]),
    (Shape::NsResize, 7, 7, [
        ".......#........",
        "......###.......",
        ".....#####......",
        ".......#........",
        ".......#........",
        ".......#........",
        ".......#........",
        ".......#........",
        ".......#........",
        ".......#........",
        ".......#........",
        ".......#........",
        ".....#####......",
        "......###.......",
        ".......#........",
        "................",
    ]),
    (Shape::Move, 7, 7, [
        ".......#........",
        "......###.......",
        ".....#.#.#......",
        ".......#........",
        ".......#........",
        "..#....#....#...",
        ".##....#....##..",
        "###############.",
        ".##....#....##..",
        "..#....#....#...",
        ".......#........",
        ".......#........",
        ".....#.#.#......",
        "......###.......",
        ".......#........",
        "................",
    ]),
    (Shape::NotAllowed, 7, 7, [
        "................",
        "......###.......",
        "....#######.....",
        "...##.....##....",
        "..###......##...",
        "..#..#......#...",
        ".##...#.....##..",
        ".##....#....##..",
        ".##.....#...##..",
        "..#......#..#...",
        "..##......###...",
        "...##.....##....",
        "....#######.....",
        "......###.......",
        "................",
        "................",
    ]),
    (Shape::Pointer, 4, 0, [
        "....##..........",
        "...#oo#.........",
        "...#oo#.........",
        "...#oo#.........",
        "...#oo###.......",
        "...#oo#oo###....",
        ".###oo#oo#oo##..",
        "#oo#oooooooo#o#.",
        "#oooooooooooooo#",
        ".#ooooooooooooo#",
        "..#oooooooooooo#",
        "..#ooooooooooo#.",
        "...#oooooooooo#.",
        "....#oooooooo#..",
        ".....########...",
        "................",
    ]),
];

fn setpix(c: &mut Cursor, x: usize, y: usize, white: bool) {
    let bit = 0x80u8 >> (x & 7);
    let i = y * 2 + (x >> 3);
    if white {
        c[8 + i] |= bit;
    } else {
        c[8 + 32 + i] |= bit;
    }
}

fn offset(c: &mut Cursor, hx: i32, hy: i32) {
    c[0..4].copy_from_slice(&(-hx.clamp(0, 15)).to_le_bytes());
    c[4..8].copy_from_slice(&(-hy.clamp(0, 15)).to_le_bytes());
}

/// a shape drawn here
pub fn shape(s: Shape) -> Cursor {
    let mut c = [0u8; 72];
    let Some((_, hx, hy, rows)) = ART.iter().find(|a| a.0 == s) else {
        return c;
    };
    let at = |x: i32, y: i32| -> u8 {
        if (0..16).contains(&x) && (0..16).contains(&y) { rows[y as usize].as_bytes()[x as usize] } else { b'.' }
    };
    for y in 0..16 {
        for x in 0..16 {
            match at(x, y) {
                b'#' => setpix(&mut c, x as usize, y as usize, false),
                ch => {
                    let edge = ch == b'o'
                        || (-1..=1).any(|dy| (-1..=1).any(|dx| at(x + dx, y + dy) == b'#'));
                    if edge {
                        setpix(&mut c, x as usize, y as usize, true);
                    }
                }
            }
        }
    }
    offset(&mut c, *hx, *hy);
    c
}

/// w x h pixels of RGBA (8 bits each): the opaque ones dark or light, every
/// scale-th pixel where it is larger than 16x16; (hx, hy) the hot spot
pub fn from_rgba(rgba: &[u8], w: usize, h: usize, hx: i32, hy: i32) -> Cursor {
    let mut c = [0u8; 72];
    let scale = w.max(h).div_ceil(16).max(1);
    for y in 0..(h / scale).min(16) {
        for x in 0..(w / scale).min(16) {
            let o = (y * scale * w + x * scale) * 4;
            let Some(p) = rgba.get(o..o + 4) else { continue };
            if p[3] >= 128 {
                let lum = p[0] as u32 * 299 + p[1] as u32 * 587 + p[2] as u32 * 114;
                setpix(&mut c, x, y, lum >= 128 * 1000);
            }
        }
    }
    offset(&mut c, hx / scale as i32, hy / scale as i32);
    c
}

static CURSORFD: Mutex<Option<Fd>> = Mutex::new(None);

/// the window's cursor: c, or the window system's arrow
pub fn set(c: Option<&Cursor>) -> std::io::Result<()> {
    let mut fd = CURSORFD.lock().unwrap();
    if fd.is_none() {
        *fd = Some(Fd::open("/dev/cursor", OWRITE)?);
    }
    let fd = fd.as_ref().unwrap();
    match c {
        Some(c) => fd.write1(c),
        None => fd.write(&[]).map(|_| ()),
    }
}
