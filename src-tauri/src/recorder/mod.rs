mod convert;

use std::fs::File;
use std::io::BufWriter;
use std::io::Cursor;
use std::io::Write;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicBool, Ordering};

use webm::mux;
use webm::mux::Track;
use xcap::Monitor;

#[derive(Debug, serde::Deserialize)]
enum Codec {
    Vp8,
    Vp9,
}

struct VPacket {
    data: Vec<u8>,
    ms: u64,
}

pub static RECORDING: AtomicBool = AtomicBool::new(false);

fn producer(monitor: Monitor, sender: Sender<VPacket>) {
    let start = Instant::now();
    let mut c = 0;
    #[allow(while_true)]
    while true {
        c += 1;
        println!("produce {}", c);

        if !RECORDING.load(Ordering::Acquire) {
            break;
        }
        let data = monitor.capture_bytes().unwrap();
        let time: Duration = Instant::now() - start;
        let ms: u64 = time.as_secs() * 1000 + time.subsec_millis() as u64;
        sender.send(VPacket { data, ms }).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(16));
    }
}

fn consumer(width: u32, height: u32, receiver: Receiver<VPacket>) {
    let mut buffer = Vec::new();
    let cursor = Cursor::new(&mut buffer);

    let mut webm: mux::Segment<mux::Writer<BufWriter<Cursor<&mut Vec<u8>>>>> =
        mux::Segment::new(mux::Writer::new(BufWriter::new(cursor)))
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
    let mut c = 0;
    while let Ok(packet) = receiver.recv() {
        c += 1;
        println!("consume {}", c);
        convert::argb_to_i420(width as usize, height as usize, &packet.data, &mut yuv);

        for f in vpx.encode(packet.ms as i64, &yuv).unwrap() {
            vt.add_frame(f.data, f.pts as u64 * 1_000_000, f.key);
        }
    }

    let mut frames = vpx.finish().unwrap();
    while let Some(frame) = frames.next().unwrap() {
        vt.add_frame(frame.data, frame.pts as u64 * 1_000_000, frame.key);
    }

    let _ = webm.finalize(None);
    // let buffer = webm.writer().into_inner().into_inner().into_inner();
    let mut file = File::create("target/current.webm").expect("fail to create file");
    file.write_all(&buffer).unwrap();
    println!("finished");
}

pub fn record(monitor: Monitor) {
    let width = monitor.width() * monitor.scale_factor() as u32;
    let height = monitor.height() * monitor.scale_factor() as u32;

    let (sender, receiver): (Sender<VPacket>, Receiver<VPacket>) = mpsc::channel();

    RECORDING.store(true, Ordering::Release);
    let producer_thread = std::thread::spawn(move || {
        producer(monitor, sender);
    });

    let consumer_thread = std::thread::spawn(move || {
        consumer(width, height, receiver);
    });

    producer_thread.join().unwrap();
    consumer_thread.join().unwrap();
}

pub fn stop_record() {
    RECORDING.store(false, Ordering::Release);
}
