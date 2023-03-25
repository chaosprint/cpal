use anyhow;
use clap::Parser;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    FromSample, Sample, SizedSample,
};

use glicol::Engine;
// use glicol_synth::{AudioContextBuilder, oscillator::SinOsc, Message};
// use std::sync::{Arc, Mutex};

// use std::boxed::Box;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, AtomicBool, AtomicPtr, Ordering};

// #[macro_use]
// extern crate lazy_static;

const BLOCK_SIZE: usize = 512;

#[derive(Parser, Debug)]
#[command(version, about = "CPAL beep example", long_about = None)]
struct Opt {
    /// The audio device to use
    #[arg(short, long, default_value_t = String::from("default"))]
    device: String,

    /// Use the JACK host
    #[cfg(all(
        any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd"
        ),
        feature = "jack"
    ))]
    #[arg(short, long)]
    #[allow(dead_code)]
    jack: bool,
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();
    
    // Conditionally compile with jack if the feature is specified.
    #[cfg(all(
        any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd"
        ),
        feature = "jack"
    ))]
    // Manually check for flags. Can be passed through cargo with -- e.g.
    // cargo run --release --example beep --features jack -- --jack
    let host = if opt.jack {
        cpal::host_from_id(cpal::available_hosts()
            .into_iter()
            .find(|id| *id == cpal::HostId::Jack)
            .expect(
                "make sure --features jack is specified. only works on OSes where jack is available",
            )).expect("jack host unavailable")
    } else {
        cpal::default_host()
    };

    #[cfg(any(
        not(any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd"
        )),
        not(feature = "jack")
    ))]
    let host = cpal::default_host();

    let device = if opt.device == "default" {
        host.default_output_device()
    } else {
        host.output_devices()?
            .find(|x| x.name().map(|y| y == opt.device).unwrap_or(false))
    }
    .expect("failed to find output device");
    println!("Output device: {}", device.name()?);

    let config = device.default_output_config().unwrap();
    println!("Default output config: {:?}", config);

    match config.sample_format() {
        cpal::SampleFormat::I8 => run::<i8>(&device, &config.into()),
        cpal::SampleFormat::I16 => run::<i16>(&device, &config.into()),
        // cpal::SampleFormat::I24 => run::<I24>(&device, &config.into()),
        cpal::SampleFormat::I32 => run::<i32>(&device, &config.into()),
        // cpal::SampleFormat::I48 => run::<I48>(&device, &config.into()),
        cpal::SampleFormat::I64 => run::<i64>(&device, &config.into()),
        cpal::SampleFormat::U8 => run::<u8>(&device, &config.into()),
        cpal::SampleFormat::U16 => run::<u16>(&device, &config.into()),
        // cpal::SampleFormat::U24 => run::<U24>(&device, &config.into()),
        cpal::SampleFormat::U32 => run::<u32>(&device, &config.into()),
        // cpal::SampleFormat::U48 => run::<U48>(&device, &config.into()),
        cpal::SampleFormat::U64 => run::<u64>(&device, &config.into()),
        cpal::SampleFormat::F32 => run::<f32>(&device, &config.into()),
        cpal::SampleFormat::F64 => run::<f64>(&device, &config.into()),
        sample_format => panic!("Unsupported sample format '{sample_format}'"),
    }
}

pub fn run<T>(device: &cpal::Device, config: &cpal::StreamConfig) -> Result<(), anyhow::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let sr = config.sample_rate.0 as usize;

    // let mut context = AudioContextBuilder::<BLOCK_SIZE>::new()
    // .sr(sr).channels(config.channels as usize).build();
    // let node_a = context.add_mono_node(SinOsc::new().sr(sr).freq(440.));
    // context.connect(node_a, context.destination);

    // let ctx = Arc::new(Mutex::new(context));
    // let channels = config.channels as usize;

    let mut engine = Engine::<BLOCK_SIZE>::new();

    let mut code = String::from(r#"
    ~t1: speed 4.0 >> seq 60 >> bd 0.2 >> mul 0.6
    
    ~t2: seq 33_33_ _33 33__33 _33
    >> sawsynth 0.01 0.1
    >> mul 0.5 >> lpf 1000.0 1.0
    
    out: mix ~t.. >> plate 0.1"#);
    let ptr = unsafe { code.as_bytes_mut().as_mut_ptr() };
    let code_ptr= Arc::new(AtomicPtr::<u8>::new(ptr));
    let code_len = Arc::new(AtomicUsize::new(code.len()));
    let has_update = Arc::new(AtomicBool::new(true));

    let _code_ptr = Arc::clone(&code_ptr);
    let _code_len = Arc::clone(&code_len);
    let _has_update = Arc::clone(&has_update);
    
    engine.set_sr(sr);
    // engine.update_with_code("o: sin 440");
    // let (b, c) = engine.next_block(vec![]);
    // println!("b {:?} c {:?}", b, c);

    let channels = 2 as usize; //config.channels as usize;

    let err_fn = |err| eprintln!("an error occurred on stream: {}", err);

    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            

            if _has_update.load(Ordering::Acquire) {
                let ptr = _code_ptr.load(Ordering::Acquire);
                let len = _code_len.load(Ordering::Acquire);
                let encoded:&[u8] = unsafe { std::slice::from_raw_parts(ptr, len) };
                let code = std::str::from_utf8(encoded.clone()).unwrap().to_owned();
                engine.update_with_code(&code);
                _has_update.store(false, Ordering::Release);
            };

            let blocks_needed = data.len() / 2 / BLOCK_SIZE;
            let block_step = channels * BLOCK_SIZE;
            for current_block in 0..blocks_needed {
                let (block, _err_msg) = engine.next_block(vec![]);
                for i in 0..BLOCK_SIZE {
                    for chan in 0..channels {
                        let value: T = T::from_sample(block[chan][i]);
                        data[(i*channels+chan)+(current_block)*block_step] = value;
                    }
                }
            }
        },
        err_fn,
        None,
    )?;
    stream.play()?;
    std::thread::sleep(std::time::Duration::from_millis(4000));

    code = String::from(r#"
    ~t1: speed 4.0 >> seq 60 >> bd 0.2 >> mul 0.6
    
    ~t2: seq 33_33_ _33 33__33 _33
    >> sawsynth 0.01 0.1
    >> mul 0.5 >> lpf 1000.0 1.0
    
    ~t3: speed 4.0 >> seq 60 61 61 63
    >> hh 0.01 >> mul 0.4
    
    out: mix ~t.. >> plate 0.1"#);
    code_ptr.store(unsafe {code.as_bytes_mut().as_mut_ptr() }, Ordering::SeqCst);
    code_len.store(code.len(), Ordering::SeqCst);
    has_update.store(true, Ordering::SeqCst);
    std::thread::sleep(std::time::Duration::from_millis(8000));
    Ok(())
}