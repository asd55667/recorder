use std::fs::File;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use fltk::enums::ColorDepth;
use fltk::{app, frame::Frame, prelude::*, window::Window};

use vpx_encode::Encoder;
use xcap::Monitor;

use webm::mux;
use webm::mux::Track;

pub mod convert;

#[derive(Debug, serde::Deserialize)]
enum Codec {
    Vp8,
    Vp9,
}

fn main() {
    if let Some(monitor) = Monitor::all().unwrap().get(1) {
        let width = monitor.width() * monitor.scale_factor() as u32;
        let height = monitor.height() * monitor.scale_factor() as u32;

        println!("name: {}", monitor.name());
        println!("factor: {}", monitor.scale_factor());
        println!("width: {}", width);
        println!("height: {}", height);

        screen_capture(monitor.clone(), width, height);
    } else {
        println!("no monitor");
        std::process::exit(0)
    }
}

fn screen_capture(monitor: Monitor, width: u32, height: u32) {
    let app = app::App::default();
    let mut window = Window::new(0, 0, 1280, 720, "Real-time Screen Capture");
    let frame = Frame::new(0, 0, window.w(), window.h(), "");

    window.end();
    window.make_resizable(true);
    window.show();

    std::thread::spawn({
        let mut frame = frame.clone();
        move || {
            let start = Instant::now();
            let out = File::create("current.webm").unwrap();
            let mut webm = mux::Segment::new(mux::Writer::new(out))
                .expect("Could not initialize the multiplexer.");

            let (vpx_codec, mux_codec) = match Codec::Vp9 {
                Codec::Vp8 => (vpx_encode::VideoCodecId::VP8, mux::VideoCodecId::VP8),
                Codec::Vp9 => (vpx_encode::VideoCodecId::VP9, mux::VideoCodecId::VP9),
            };

            let mut vpx = vpx_encode::Encoder::new(vpx_encode::Config {
                width: width,
                height: height,
                timebase: [1, 1000],
                bitrate: 5000,
                codec: vpx_codec,
            })
            .unwrap();

            let mut vt = webm.add_video_track(width, height, None, mux_codec);
            let mut yuv: Vec<_> = Vec::new();

            #[allow(while_true)]
            while true {
                // TODO: bgr2rgb
                let data = &monitor.capture_bytes().unwrap();

                let img = fltk::image::RgbImage::new(
                    data,
                    width as i32,
                    height as i32,
                    ColorDepth::Rgba8,
                )
                .unwrap();
                img.clone().scale(window.w(), window.h(), true, true);

                frame.set_image(Some(img));

                app::sleep(0.016);
                app::awake();

                frame.redraw();

                convert::argb_to_i420(width as usize, height as usize, data, &mut yuv);

                let time: Duration = Instant::now() - start;
                if time > Duration::from_secs(10) {
                    break;
                }
                let ms = time.as_secs() * 1000 + time.subsec_millis() as u64;
                for f in vpx.encode(ms as i64, &yuv).unwrap() {
                    vt.add_frame(f.data, f.pts as u64 * 1_000_000, f.key);
                }
            }

            let mut frames = vpx.finish().unwrap();
            while let Some(frame) = frames.next().unwrap() {
                vt.add_frame(frame.data, frame.pts as u64 * 1_000_000, frame.key);
            }

            let _ = webm.finalize(None);
        }
    });

    app.run().unwrap();
}
