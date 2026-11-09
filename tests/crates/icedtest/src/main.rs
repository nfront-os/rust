// SPDX-License-Identifier: MIT
//
// icedtest: iced on 9front: winit (its plan9 backend), the tiny-skia renderer
// on the processor into softbuffer (its plan9 backend: draw(3)), the
// clipboard through window_clipboard (its plan9 backend: /dev/snarf), in a
// window of wsys or rio. Checks that frames reach the window (/dev/window:
// the theme's background, the text on it) and the clipboard both ways
// (iced's write, then /dev/snarf; /dev/snarf, then iced's read).
// -e secs: then a button (most of the window: the host clicks its middle)
// and a text field under the title (the host clicks into it, then types:
// tests/winit/run.sh), a line each click and each change of the text.
// Prints "ok ..." / "FAIL ..." lines and a summary.
//
// Open: a click on the button is not taken (the press is, its release finds
// the button no longer pressed); the text field takes clicks and keys.
//
// Built here (cargo --config ../../../cargo-plan9.toml: the crates with plan9
// backends), run by 9front's tests/winit/run.sh in a window of wsys.
//
// usage: icedtest [-e secs]

use std::time::{Duration, Instant};

use iced::widget::{button, column, text, text_input};
use iced::{Element, Length, Subscription, Task, Theme};

mod common;
use common::Report;

#[derive(Debug, Clone)]
enum Msg {
    Trace(String), // ICEDTRACE: the mouse's and the window's events as iced has them
    Frame,
    Press,
    Input(String),
    Read(Option<String>),
}

struct App {
    r: Report,
    frames: u32,
    evsecs: u64,
    start: Option<Instant>,
    clicks: u32,
    text: String,
    stage: u32,
}

const FIELD: &str = "field";

impl App {
    fn new() -> (App, Task<Msg>) {
        let args: Vec<String> = std::env::args().collect();
        let evsecs = if args.len() > 2 && args[1] == "-e" { args[2].parse().unwrap_or(0) } else { 0 };
        let app = App { r: Report::default(), frames: 0, evsecs, start: None, clicks: 0, text: String::new(), stage: 0 };
        (app, Task::none())
    }

    /// the theme's background in the window, text on it
    fn check(&mut self) {
        let bg = Theme::Dark.palette().background.into_rgba8();
        let want = (bg[0] as u32) << 16 | (bg[1] as u32) << 8 | bg[2] as u32;
        common::check(&mut self.r, "iced's background", 4, 200, want);
        let mut other = 0;
        for x in (4..150).step_by(2) {
            for y in 4..30 {
                if matches!(common::pixel(x, y), Ok(p) if p != want) {
                    other += 1;
                }
            }
        }
        if other > 20 {
            self.r.ok(&format!("iced's text drawn: {other} pixels of it"));
        } else {
            self.r.fail(&format!("iced's text: {other} pixels of it"));
        }
    }

    fn done(&mut self) -> Task<Msg> {
        if self.evsecs > 0 {
            println!("clicks {} text {:?}", self.clicks, self.text);
        }
        let code = self.r.summary("icedtest");
        std::process::exit(code);
    }

    fn update(&mut self, m: Msg) -> Task<Msg> {
        match m {
            Msg::Frame => {
                self.frames += 1;
                match self.stage {
                    0 if self.frames >= 10 => {
                        std::thread::sleep(Duration::from_millis(100));
                        self.r.ok(&format!("{} frames", self.frames));
                        self.check();
                        self.stage = 1;
                        return iced::clipboard::write("icedtest snarf".to_string());
                    },
                    1 => {
                        // iced's write, through /dev/snarf
                        match std::fs::read_to_string("/dev/snarf") {
                            Ok(s) if s == "icedtest snarf" => self.r.ok("clipboard: iced's write in /dev/snarf"),
                            Ok(s) => self.r.fail(&format!("clipboard: /dev/snarf has {s:?}")),
                            Err(e) => self.r.fail(&format!("clipboard: /dev/snarf: {e}")),
                        }
                        let _ = std::fs::OpenOptions::new()
                            .write(true)
                            .truncate(true)
                            .open("/dev/snarf")
                            .and_then(|mut f| std::io::Write::write_all(&mut f, b"from snarf"));
                        self.stage = 2;
                        return iced::clipboard::read().map(Msg::Read);
                    },
                    3 if self.evsecs == 0 => return self.done(),
                    3 => {
                        self.start = Some(Instant::now());
                        self.stage = 4;
                        println!("ready for events");
                    },
                    4 if self.start.is_some_and(|s| s.elapsed() > Duration::from_secs(self.evsecs)) => {
                        return self.done();
                    },
                    _ => {},
                }
            },
            Msg::Read(s) => {
                if s.as_deref() == Some("from snarf") {
                    self.r.ok("clipboard: /dev/snarf read by iced");
                } else {
                    self.r.fail(&format!("clipboard: iced read {s:?}"));
                }
                self.stage = 3;
            },
            Msg::Trace(t) => println!("trace {t}"),
            Msg::Press => {
                self.clicks += 1;
                println!("event clicked {}", self.clicks);
            },
            Msg::Input(s) => {
                self.text = s;
                println!("event text {}", self.text);
            },
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Msg> {
        column![
            text(format!("iced on 9front: {} clicks", self.clicks)).size(20),
            text_input("type here", &self.text).id(FIELD).on_input(Msg::Input),
            button("press me").on_press(Msg::Press).width(Length::Fill).height(Length::Fill),
        ]
        .spacing(10)
        .padding(8)
        .into()
    }

    fn subscription(&self) -> Subscription<Msg> {
        let frames = iced::window::frames().map(|_| Msg::Frame);
        if std::env::var_os("ICEDTRACE").is_none() {
            return frames;
        }
        Subscription::batch([frames, iced::event::listen_with(trace)])
    }
}

/// ICEDTRACE: the mouse's and the window's events (but redraws), whether a widget took them
fn trace(ev: iced::Event, status: iced::event::Status, _: iced::window::Id) -> Option<Msg> {
    match &ev {
        iced::Event::Mouse(m) => Some(Msg::Trace(format!("{m:?} {status:?}"))),
        iced::Event::Window(iced::window::Event::RedrawRequested(_)) => None,
        iced::Event::Window(w) => Some(Msg::Trace(format!("{w:?}"))),
        _ => None,
    }
}

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .subscription(App::subscription)
        .theme(|_: &App| Theme::Dark)
        .title("icedtest")
        .run()
}
