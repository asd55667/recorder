mod convert;

use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufWriter;
use std::io::Cursor;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crossbeam::channel::{unbounded, Receiver, Sender};
use webm::mux;
use webm::mux::Track;
use xcap::Monitor;

use num_cpus;

#[derive(Debug, serde::Deserialize)]
enum Codec {
    Vp8,
    Vp9,
}

struct VPacket {
    data: Vec<u8>,
    ms: u64,
    seq: u64, // Add sequence number
}

pub static RECORDING: AtomicBool = AtomicBool::new(false);

fn producer(monitor: Monitor, sender: Sender<VPacket>) {
    let start = Instant::now();
    let mut seq = 0; // Initialize sequence counter
    while RECORDING.load(Ordering::Acquire) {
        // Changed while true to while RECORDING

        println!("produce {}", seq);

        if !RECORDING.load(Ordering::Acquire) {
            break;
        }
        let data = monitor.capture_bytes().unwrap();
        let time: Duration = Instant::now() - start;
        let ms: u64 = time.as_secs() * 1000 + time.subsec_millis() as u64;
        sender.send(VPacket { data, ms, seq }).unwrap();

        seq += 1;
        std::thread::sleep(std::time::Duration::from_millis(16));
    }

    // Producer explicitly drops sender when done
    drop(sender);
}

// Add converter function that will run in parallel
fn converter(width: usize, height: usize, receiver: Receiver<VPacket>, sender: Sender<VPacket>) {
    let mut yuv = Vec::new();
    while let Ok(packet) = receiver.recv() {
        convert::argb_to_i420(width, height, &packet.data, &mut yuv);
        if let Err(_) = sender.send(VPacket {
            data: yuv.clone(),
            ms: packet.ms,
            seq: packet.seq, // Preserve sequence number
        }) {
            println!("convert fail {}", packet.seq);
            break;
        }
    }
    // Converter explicitly drops sender when done
    drop(sender);
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

    let mut next_seq = 0; // Track expected sequence number
    let mut pending_packets: BTreeMap<u64, VPacket> = BTreeMap::new();
    while let Ok(packet) = receiver.recv() {
        // Store out-of-order packets
        pending_packets.insert(packet.seq, packet);

        while let Some((&seq, packet)) = pending_packets.first_key_value() {
            if seq != next_seq {
                break;
            }

            println!("consume {}", next_seq);
            let packet = pending_packets.remove(&seq).unwrap();

            for f in vpx.encode(packet.ms as i64, &packet.data).unwrap() {
                vt.add_frame(f.data, f.pts as u64 * 1_000_000, f.key);
            }

            next_seq += 1;
        }
    }

    let mut frames = vpx.finish().unwrap();
    while let Some(frame) = frames.next().unwrap() {
        vt.add_frame(frame.data, frame.pts as u64 * 1_000_000, frame.key);
    }

    let _ = webm.finalize(None);
    let mut file = File::create("target/current.webm").expect("fail to create file");
    file.write_all(&buffer).unwrap();
    println!("finished.")
}

pub fn record(monitor: Monitor) {
    let width = monitor.width() * monitor.scale_factor() as u32;
    let height = monitor.height() * monitor.scale_factor() as u32;

    let (sender, receiver) = unbounded();
    let (converted_sender, converted_receiver) = unbounded();

    RECORDING.store(true, Ordering::Release);
    let producer_thread = std::thread::spawn(move || {
        producer(monitor, sender);
    });

    // Start parallel converter threads
    let num_converters = num_cpus::get().max(2) - 1; // Use available CPU cores minus 1
    // println!("num_converters, {}", num_converters);
    let mut converter_threads = Vec::new();

    for _ in 0..num_converters {
        let receiver = receiver.clone();
        let sender = converted_sender.clone();

        let converter_thread = std::thread::spawn(move || {
            converter(width as usize, height as usize, receiver, sender);
        });
        converter_threads.push(converter_thread);
    }

    // Drop the extra sender we created from the clones
    drop(converted_sender);

    let consumer_thread = std::thread::spawn(move || {
        consumer(width, height, converted_receiver);
    });

    producer_thread.join().unwrap();
    for thread in converter_threads {
        thread.join().unwrap();
    }
    consumer_thread.join().unwrap();
}

pub fn stop_record() {
    RECORDING.store(false, Ordering::Release);
}
