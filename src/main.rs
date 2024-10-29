use fltk::enums::ColorDepth;
use fltk::{app, frame::Frame, prelude::*, window::Window};

use xcap::Monitor;

fn main() {
    if let Some(monitor) = Monitor::all().unwrap().get(0) {
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
    let mut frame = Frame::new(0, 0, window.w(), window.height(), "");

    window.end();
    window.make_resizable(true);
    window.show();

    let data = monitor.clone().capture_image().unwrap();
    // data.save(format!("target/monitor.png")).unwrap();
    let data = data.as_raw();

    let mut img =
        fltk::image::RgbImage::new(data, width as i32, height as i32, ColorDepth::Rgba8).unwrap();
    img.scale(window.w(), window.h(), true, true);
    frame.set_image(Some(img));

    app.run().unwrap();
}
