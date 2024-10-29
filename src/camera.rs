use opencv::{highgui, prelude::*, videoio, Result};

fn video_capture() -> Result<()> {
    // Open the default camera (camera index 0)
    let mut camera = videoio::VideoCapture::new(0, videoio::CAP_ANY)?;

    if !videoio::VideoCapture::is_opened(&camera)? {
        panic!("Could not open the camera.");
    }

    let window_name = "Camera Capture";
    highgui::named_window(window_name, highgui::WINDOW_AUTOSIZE)?;

    loop {
        let mut frame = Mat::default();
        camera.read(&mut frame)?;

        if frame.size()?.width > 0 {
            highgui::imshow(window_name, &frame)?;
        }

        // Exit on ESC key
        if highgui::wait_key(10)? == 27 {
            break;
        }
    }

    Ok(())
}
