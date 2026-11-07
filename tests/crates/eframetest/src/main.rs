// SPDX-License-Identifier: MIT
//
// eframetest: egui's own framework on 9front: eframe through winit (its
// plan9 backend) and wgpu (its Vulkan backend: Mesa's libvulkan.a,
// VK_PLAN9_surface), or without a GPU on the processor (eframe's feature
// software: egui_software_backend into softbuffer), in a window of wsys or
// rio. Says which painter eframe took (-r: the one it must take), draws some
// frames and checks that they reach the window (/dev/window: egui's panel
// colour, and text on it) and a screenshot of egui's, then closes it.
// Prints "ok ..." / "FAIL ..." lines and a summary.
//
// Built in ../9front-rust/tests/crates/eframetest (cargo --config cargo-plan9.toml),
// linked with -l draw -l 9 -l fmt -l utf -l stdc++ (Mesa's libvulkan.a);
// --no-default-features: without wgpu (no Vulkan: the processor only).
//
// usage: eframetest [-r wgpu|software] [frames]
// env: EFRAME_RENDERER=software|wgpu: eframe's painter (default: wgpu, the processor without an adapter)
//      EFRAMETEST_LOG=error|warn|info|debug|trace: the log of wgpu, eframe, ... on stderr
//      EFRAMETEST_PRESENT=fifo|mailbox|immediate: wgpu's present mode (default: AutoVsync)

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;

mod common;
use common::Report;

struct App {
    r: Arc<Mutex<Report>>,
    frames: u32,
    want: u32,
    t0: Option<Instant>,
    checked: bool,
    /// -r: the painter eframe must take
    renderer: Option<String>,
    shot: Arc<Mutex<Option<Arc<egui::ColorImage>>>>,
    asked: bool,
}

impl App {
    fn check(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let mut r = self.r.lock().unwrap();
        #[cfg(feature = "wgpu")]
        let got = if let Some(rs) = frame.wgpu_render_state() {
            let i = rs.adapter.get_info();
            r.ok(&format!("wgpu adapter: {} ({:?}, {:?})", i.name, i.backend, i.device_type));
            if i.backend != eframe::wgpu::Backend::Vulkan {
                r.fail(&format!("wgpu backend {:?}, want Vulkan", i.backend));
            }
            "wgpu"
        } else {
            "software"
        };
        #[cfg(not(feature = "wgpu"))]
        let got = {
            let _ = frame;
            "software"
        };
        match &self.renderer {
            Some(want) if want != got => r.fail(&format!("painter {got}, want {want}")),
            _ => r.ok(&format!("painter {got}")),
        }
        let el = self.t0.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0);
        r.ok(&format!("{} frames in {:.2} s", self.frames, el));
        let size = ctx.content_rect().size();
        let bg = ctx.global_style().visuals.panel_fill;
        let want = (bg.r() as u32) << 16 | (bg.g() as u32) << 8 | bg.b() as u32;
        common::check(&mut r, "egui's panel", size.x as i32 - 4, size.y as i32 - 4, want);
        let mut other = 0;
        for x in (8..200).step_by(2) {
            for y in 8..30 {
                if matches!(common::pixel(x, y), Ok(p) if p != want) {
                    other += 1;
                }
            }
        }
        if other > 20 {
            r.ok(&format!("egui's heading drawn: {other} pixels of text"));
        } else {
            r.fail(&format!("egui's heading: {other} pixels of text"));
        }
        // egui's screenshot (ViewportCommand::Screenshot): the panel's colour
        match self.shot.lock().unwrap().as_ref() {
            Some(img) => {
                let [w, h] = img.size;
                let p = img.pixels[(h - 4) * w + w - 4];
                let got = (p.r() as u32) << 16 | (p.g() as u32) << 8 | p.b() as u32;
                if got == want {
                    r.ok(&format!("screenshot {w}x{h}, the panel's colour"));
                } else {
                    r.fail(&format!("screenshot {w}x{h}: {got:06x}, want {want:06x}"));
                }
            }
            None => r.fail("no screenshot"),
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("eframe on 9front");
            ui.label(format!("frame {}", self.frames));
            let _ = ui.button("a button");
        });
        if self.t0.is_none() {
            self.t0 = Some(Instant::now());
        }
        self.frames += 1;
        if std::env::var_os("EFRAMETEST_LOG").is_some() {
            eprintln!("frame {}", self.frames);
        }
        let ctx = ui.ctx().clone();
        if self.frames + 5 >= self.want && !self.asked {
            self.asked = true;
            let shot = self.shot.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(
                move |img| *shot.lock().unwrap() = Some(img),
            )));
        }
        if self.frames < self.want {
            ctx.request_repaint();
        } else if !self.checked {
            self.checked = true;
            // what was presented last is on the screen by now
            std::thread::sleep(Duration::from_millis(200));
            self.check(&ctx, frame);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

/// what wgpu and eframe log, on stderr
struct Logger;

impl log::Log for Logger {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, r: &log::Record<'_>) {
        if self.enabled(r.metadata()) {
            eprintln!("{} {}: {}", r.level(), r.target(), r.args());
        }
    }
    fn flush(&self) {}
}

fn main() {
    if let Ok(l) = std::env::var("EFRAMETEST_LOG") {
        if let Ok(l) = l.parse() {
            log::set_logger(&Logger).ok();
            log::set_max_level(l);
        }
    }
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut renderer = None;
    if args.len() >= 2 && args[0] == "-r" {
        renderer = Some(args[1].clone());
        args.drain(..2);
    }
    let want = args.first().and_then(|a| a.parse().ok()).unwrap_or(60);
    let r = Arc::new(Mutex::new(Report::default()));
    #[cfg(feature = "wgpu")]
    let mut wgpu_options = eframe::egui_wgpu::WgpuConfiguration::default();
    #[cfg(feature = "wgpu")]
    match std::env::var("EFRAMETEST_PRESENT").as_deref() {
        Ok("fifo") => wgpu_options.surface.present_mode = eframe::wgpu::PresentMode::Fifo,
        Ok("mailbox") => wgpu_options.surface.present_mode = eframe::wgpu::PresentMode::Mailbox,
        Ok("immediate") => wgpu_options.surface.present_mode = eframe::wgpu::PresentMode::Immediate,
        _ => {}
    }
    let opts = eframe::NativeOptions {
        // the default: wgpu, the processor when it finds no adapter
        #[cfg(feature = "wgpu")]
        wgpu_options,
        viewport: egui::ViewportBuilder::default().with_title("eframetest"),
        ..Default::default()
    };
    let rr = r.clone();
    let res = eframe::run_native(
        "eframetest",
        opts,
        Box::new(move |_cc| Ok(Box::new(App {
            r: rr,
            frames: 0,
            want,
            t0: None,
            checked: false,
            renderer,
            shot: Default::default(),
            asked: false,
        }))),
    );
    let mut r = r.lock().unwrap();
    match res {
        Ok(()) => r.ok("eframe returned"),
        Err(e) => r.fail(&format!("eframe: {e}")),
    }
    // eframetest or eframetestsw (without wgpu)
    let name = std::env::args().next().unwrap_or_default();
    let name = name.rsplit('/').next().unwrap_or("eframetest");
    std::process::exit(r.summary(name));
}
