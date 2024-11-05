mod convert;

use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufWriter;
use std::io::Cursor;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::prelude::*;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    BufferSize, Device, Host, InputCallbackInfo, StreamConfig, SupportedStreamConfig,
};
use crossbeam::channel::{unbounded, Receiver, Sender};
use num_cpus;
use opus::{Application::*, Channels, Channels::*, Encoder};
use ringbuf::{traits::*, HeapRb, SharedRb};
use webm::mux;
use webm::mux::Track;
use xcap::Monitor;

use anyhow::{anyhow, bail, Context};
pub type ResultType<F, E = anyhow::Error> = anyhow::Result<F, E>;

#[derive(Debug, serde::Deserialize)]
enum Codec {
    Vp8,
    Vp9,
}

// Add this struct to store audio configuration
#[derive(Debug, Clone)] // Add Clone derive
pub struct AudioConfig {
    pub sample_rate: u32,
    pub sample_rate_0: u32,
    pub device_channel: u16,
    pub encode_channel: Channels,
}

struct AVPacket {
    audio_data: Vec<f32>,
    video_data: Vec<u8>,
    ms: u64,
    seq: u64, // Add sequence number
}

lazy_static::lazy_static! {
    static ref HOST: Host = cpal::default_host();
    static ref INPUT_BUFFER: Arc<Mutex<std::collections::VecDeque<f32>>> = Default::default();
    static ref VOICE_CALL_INPUT_DEVICE: Arc::<Mutex::<Option<String>>> = Default::default();
}
pub static RECORDING: AtomicBool = AtomicBool::new(false);

fn setup_audio() -> ResultType<(
    impl StreamTrait,
    AudioConfig,
    ringbuf::wrap::caching::Caching<
        std::sync::Arc<SharedRb<ringbuf::storage::Heap<f32>>>,
        false,
        true,
    >,
)> {
    use cpal::SampleFormat::*;

    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .expect("no input device available");

    let config = device
        .default_input_config()
        .expect("no default input config");

    let sample_rate_0 = config.sample_rate().0;
    let device_channel = config.channels();

    let encode_channel = if device_channel > 1 { Stereo } else { Mono };
    // Sample rate must be one of 8000, 12000, 16000, 24000, or 48000.
    let sample_rate = if sample_rate_0 < 12000 {
        8000
    } else if sample_rate_0 < 16000 {
        12000
    } else if sample_rate_0 < 24000 {
        16000
    } else if sample_rate_0 < 48000 {
        24000
    } else {
        48000
    };
    // Create a ring buffer with capacity for 1 second of audio
    let rb = HeapRb::<f32>::new(sample_rate as usize * device_channel as usize);
    let (mut producer, consumer) = rb.split();

    let stream = match config.sample_format() {
        I8 => build_input_stream::<i8>(producer, device, &config, encode_channel)?,
        I16 => build_input_stream::<i16>(producer, device, &config, encode_channel)?,
        I32 => build_input_stream::<i32>(producer, device, &config, encode_channel)?,
        I64 => build_input_stream::<i64>(producer, device, &config, encode_channel)?,
        U8 => build_input_stream::<u8>(producer, device, &config, encode_channel)?,
        U16 => build_input_stream::<u16>(producer, device, &config, encode_channel)?,
        U32 => build_input_stream::<u32>(producer, device, &config, encode_channel)?,
        U64 => build_input_stream::<u64>(producer, device, &config, encode_channel)?,
        F32 => build_input_stream::<f32>(producer, device, &config, encode_channel)?,
        F64 => build_input_stream::<f64>(producer, device, &config, encode_channel)?,
        f => bail!("unsupported audio format: {:?}", f),
    };

    Ok((
        stream,
        AudioConfig {
            sample_rate_0,
            sample_rate,
            device_channel,
            encode_channel: encode_channel as _,
        },
        consumer,
    ))
}

fn build_input_stream<T>(
    producer: ringbuf::wrap::caching::Caching<
        Arc<SharedRb<ringbuf::storage::Heap<f32>>>,
        true,
        false,
    >,
    device: Device,
    config: &SupportedStreamConfig,
    encode_channel: Channels,
) -> ResultType<cpal::Stream>
where
    T: cpal::SizedSample + dasp::sample::ToSample<f32>,
{
    let err_fn = move |err| {
        // too many UnknownErrno, will improve later
        // log::trace!("an error occurred on stream: {}", err);
    };
    let sample_rate_0 = config.sample_rate().0;
    // log::debug!("Audio sample rate : {}", sample_rate);
    unsafe {
        AUDIO_ZERO_COUNT = 0;
    }
    let device_channel = config.channels();
    // https://www.opus-codec.org/docs/html_api/group__opusencoder.html#gace941e4ef26ed844879fde342ffbe546
    // https://chromium.googlesource.com/chromium/deps/opus/+/1.1.1/include/opus.h
    // Do not set `frame_size = sample_rate as usize / 100;`
    // Because we find `sample_rate as usize / 100` will cause encoder error in `encoder.encode_vec_float()` sometimes.
    // https://github.com/xiph/opus/blob/2554a89e02c7fc30a980b4f7e635ceae1ecba5d6/src/opus_encoder.c#L725
    let frame_size = sample_rate_0 as usize / 100; // 10 ms
    let encode_len = frame_size * encode_channel as usize;
    let rechannel_len = encode_len * device_channel as usize / encode_channel as usize;
    INPUT_BUFFER.lock().unwrap().clear();
    let timeout = None;
    let stream_config = StreamConfig {
        channels: device_channel,
        sample_rate: config.sample_rate(),
        buffer_size: BufferSize::Default,
    };
    let mut producer = producer;
    let stream = device.build_input_stream(
        &stream_config,
        move |data: &[T], _: &InputCallbackInfo| {
            let buffer: Vec<f32> = data.iter().map(|s| T::to_sample(*s)).collect();
            let mut lock = INPUT_BUFFER.lock().unwrap();
            lock.extend(buffer);
            while lock.len() >= rechannel_len {
                let frame: Vec<f32> = lock.drain(0..rechannel_len).collect();
                producer.push_slice(&frame);
            }
        },
        err_fn,
        timeout,
    )?;

    Ok(stream)
}

fn producer(
    monitor: Monitor,
    audio_consumer: ringbuf::wrap::caching::Caching<
        std::sync::Arc<SharedRb<ringbuf::storage::Heap<f32>>>,
        false,
        true,
    >,
    audio_config: AudioConfig,
    sender: Sender<AVPacket>,
) {
    let fps = 60.0;
    let frame_duration = 1000.0 / fps;
    let start = Instant::now();
    let mut seq = 0; // Initialize sequence counter

    // Calculate how many audio samples we want per frame
    let samples_per_frame =
        ((audio_config.sample_rate as f32 / fps) as usize) * audio_config.encode_channel as usize;

    let mut audio_buffer: Vec<f32> = Vec::with_capacity(samples_per_frame);

    let mut consumer = audio_consumer;
    while RECORDING.load(Ordering::Acquire) {
        // println!("produce {}", seq);

        let video_data = monitor.capture_bytes().unwrap();

        // Collect audio samples
        audio_buffer.clear();
        while audio_buffer.len() < samples_per_frame as usize {
            audio_buffer.extend(consumer.pop_iter());
            // let received = consumer.pop_iter();
            // if received.len() > 0 {
            //     audio_buffer.extend(received);
            // } else {
            //     audio_buffer.push(0.0)
            // }
        }

        let time: Duration = Instant::now() - start;
        let ms: u64 = time.as_secs() * 1000 + time.subsec_millis() as u64;

        sender
            .send(AVPacket {
                video_data,
                audio_data: audio_buffer.clone(),
                ms,
                seq,
            })
            .unwrap();

        seq += 1;
        std::thread::sleep(std::time::Duration::from_millis(frame_duration as u64));
    }

    // Producer explicitly drops sender when done
    drop(sender);
}

// Add converter function that will run in parallel
fn converter(
    audio_config: AudioConfig,
    width: usize,
    height: usize,
    receiver: Receiver<AVPacket>,
    sender: Sender<AVPacket>,
) {
    let mut yuv = Vec::new();
    while let Ok(packet) = receiver.recv() {
        convert::argb_to_i420(width, height, &packet.video_data, &mut yuv);

        let mut audio_data = packet.audio_data;

        let sample_rate = audio_config.sample_rate;
        let sample_rate0 = audio_config.sample_rate_0;
        let device_channel = audio_config.device_channel;
        let encode_channel = audio_config.encode_channel as _;
        if sample_rate0 != sample_rate {
            audio_data =
                convert::audio_resample(&audio_data, sample_rate0, sample_rate, device_channel);
        }
        if device_channel != encode_channel {
            audio_data = convert::audio_rechannel(
                audio_data,
                sample_rate,
                sample_rate,
                device_channel,
                encode_channel,
            )
        }

        if let Err(_) = sender.send(AVPacket {
            video_data: yuv.clone(),
            audio_data: audio_data,
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

fn consumer(width: u32, height: u32, audio_config: AudioConfig, receiver: Receiver<AVPacket>) {
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

    let mut opus_encoder = Encoder::new(
        audio_config.sample_rate,
        audio_config.encode_channel,
        LowDelay,
    )
    .unwrap();
    // Calculate the frame size that Opus expects
    // Opus supports specific frame sizes: 2.5, 5, 10, 20, 40, or 60 ms
    let supported_frame_duration = vec![2.5, 5.0, 10.0, 20.0, 40.0, 60.0];
    let rate = audio_config.sample_rate * audio_config.encode_channel as u32;
    let default_frame_duration = supported_frame_duration.get(4).unwrap();
    let default_frame_size = (rate as f64 * *default_frame_duration * 0.001) as u64;
    let rate = audio_config.sample_rate * audio_config.encode_channel as u32;
    let mut supported_frame_size = vec![];
    for duration in supported_frame_duration.iter() {
        supported_frame_size.push((rate as f64 * duration * 0.001) as u64);
    }

    opus_encoder
        .set_bitrate(opus::Bitrate::Bits(128000))
        .unwrap();

    let mut vt = webm.add_video_track(width, height, None, mux_codec);
    let mut at = webm.add_audio_track(
        audio_config.sample_rate as i32,
        audio_config.device_channel as i32,
        None,
        mux::AudioCodecId::Opus,
    );

    let silent_frame: Vec<f32> = vec![0.0; default_frame_size as usize];
    // Pre-encode a silent frame to use when needed
    let encoded_silence = opus_encoder
        .encode_vec_float(&silent_frame, silent_frame.len())
        .unwrap_or_default();

    let mut next_seq = 0; // Track expected sequence number
    let mut pending_packets: BTreeMap<u64, AVPacket> = BTreeMap::new();
    while let Ok(packet) = receiver.recv() {
        // Store out-of-order packets
        pending_packets.insert(packet.seq, packet);

        while let Some((&seq, packet)) = pending_packets.first_key_value() {
            if seq != next_seq {
                break;
            }

            // println!("consume {}", next_seq);
            let packet = pending_packets.remove(&seq).unwrap();

            for f in vpx.encode(packet.ms as i64, &packet.video_data).unwrap() {
                vt.add_frame(f.data, f.pts as u64 * 1_000_000, f.key);
            }

            // Encode audio data with opus encoder
            if !packet.audio_data.is_empty() {
                let data = &packet.audio_data;
                // For audio frames, we can check if the sequence number is 0 (first frame)
                // or if we're at a regular keyframe interval (e.g. every 60 frames for 1 second at 60fps)
                let is_keyframe = seq == 0 || seq % 60 == 0;

                if let Some(support) = supported_frame_size
                    .iter()
                    .find(|x| **x == data.len() as u64)
                {
                    match opus_encoder.encode_vec_float(data, data.len() * 6) {
                        Ok(data) => {
                            at.add_frame(&data, packet.ms * 1_000_000, is_keyframe);
                            println!("success {}, {}", next_seq, packet.audio_data.len());
                        }
                        Err(e) => {
                            at.add_frame(&encoded_silence, packet.ms * 1_000_000, false);
                            eprintln!(
                                "Frame {}, data: {}: Audio encoding error: {:?}",
                                next_seq,
                                data.len(),
                                e
                            )
                        }
                    }
                } else {
                    at.add_frame(&encoded_silence, packet.ms * 1_000_000, false);
                }
            }

            next_seq += 1;
        }
    }

    let mut frames = vpx.finish().unwrap();
    while let Some(frame) = frames.next().unwrap() {
        vt.add_frame(frame.data, frame.pts as u64 * 1_000_000, frame.key);
    }

    let _ = webm.finalize(None);
    let now = Utc::now();
    let formatted_time = now.format("%Y-%m-%d %H:%M:%S").to_string();
    let filename = format!("target/{formatted_time}.webm");
    let mut file = File::create(filename).expect("fail to create file");
    file.write_all(&buffer).unwrap();
    println!("finished.")
}

pub fn record(monitor: Monitor) {
    let width = monitor.width() * monitor.scale_factor() as u32;
    let height = monitor.height() * monitor.scale_factor() as u32;

    let (sender, receiver) = unbounded();
    let (converted_sender, converted_receiver) = unbounded();

    // Setup audio capture
    let (audio_stream, audio_config, audio_consumer) = setup_audio().unwrap();

    RECORDING.store(true, Ordering::Release);
    let producer_audio_config = audio_config.clone();
    let producer_thread = std::thread::spawn(move || {
        producer(monitor, audio_consumer, producer_audio_config, sender);
    });

    // Start parallel converter threads
    let num_converters = num_cpus::get().max(2) - 1; // Use available CPU cores minus 1
                                                     // println!("num_converters, {}", num_converters);
    let mut converter_threads = Vec::new();
    for _ in 0..num_converters {
        let receiver = receiver.clone();
        let sender = converted_sender.clone();

        let converter_audio_config = audio_config.clone();
        let converter_thread = std::thread::spawn(move || {
            converter(
                converter_audio_config,
                width as usize,
                height as usize,
                receiver,
                sender,
            );
        });
        converter_threads.push(converter_thread);
    }

    // Drop the extra sender we created from the clones
    drop(converted_sender);

    let audio_config = audio_config.clone();
    let consumer_thread = std::thread::spawn(move || {
        consumer(width, height, audio_config, converted_receiver);
    });

    producer_thread.join().unwrap();
    for thread in converter_threads {
        thread.join().unwrap();
    }
    consumer_thread.join().unwrap();
    audio_stream.play().expect("Failed to start audio stream");
}

#[inline]
pub fn stop_record() {
    RECORDING.store(false, Ordering::Release);
}

const MAX_AUDIO_ZERO_COUNT: u16 = 800;
static mut AUDIO_ZERO_COUNT: u16 = 0;

fn get_device() -> ResultType<(Device, SupportedStreamConfig)> {
    let audio_input = get_audio_input();
    _get_audio_input(&audio_input)
}

fn _get_audio_input(audio_input: &str) -> ResultType<(Device, SupportedStreamConfig)> {
    let mut device = None;
    if !audio_input.is_empty() {
        for d in HOST
            .devices()
            .with_context(|| "Failed to get audio devices")?
        {
            if d.name().unwrap_or("".to_owned()) == audio_input {
                device = Some(d);
                break;
            }
        }
    }
    let device = device.unwrap_or(
        HOST.default_input_device()
            .with_context(|| "Failed to get default input device for loopback")?,
    );
    // log::info!("Input device: {}", device.name().unwrap_or("".to_owned()));
    let format = device
        .default_input_config()
        .map_err(|e| anyhow!(e))
        .with_context(|| "Failed to get default input format")?;
    // log::info!("Default input format: {:?}", format);
    Ok((device, format))
}

fn get_audio_input() -> String {
    VOICE_CALL_INPUT_DEVICE.lock().unwrap().clone().unwrap()
}
