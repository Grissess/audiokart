extern crate analysis;

extern crate rodio;

use std::fs::File;
use std::io::BufReader;
use std::time::{Duration, Instant};
use analysis::{Timecode, BeatInfo, WindowInfo};
use rodio::Source;

fn dur_to_frac(d: Duration) -> f64 {
    (d.as_secs() as f64) + 1e-9f64 * (d.subsec_nanos() as f64)
}

enum EventData {
    Beat(bool, Vec<bool>),
    Stop,
}

impl EventData {
    fn pretty_print(&self) {
        match *self {
            EventData::Beat(master, ref bands) => {
                if master {
                    print!("* ");
                } else {
                    print!("  ");
                }
                for (idx, band) in bands.iter().enumerate() {
                    print!("\x1b[0;{};{}m",
                        if (idx % 14) >= 7 { 1 } else { 2 },
                        (idx % 7) + 31,
                    );
                    if *band {
                        print!("*");
                    } else {
                        print!(" ");
                    }
                }
                println!("\x1b[m");
            },
            EventData::Stop => println!("done"),
        }
    }
}

struct Event {
    pub time: Duration,
    pub data: EventData,
}

struct DebugListener(Vec<Event>);

impl analysis::AnalysisListener for DebugListener {
    fn start(&mut self, rate: usize, bands: usize, window: usize) {
        println!("Sample rate is {}; bands {}, window {}", rate, bands, window);
    }

    fn stop(&mut self, time: Timecode) {
        self.0.push(Event { time: time.into(), data: EventData::Stop });
    }

    fn beat(&mut self, time: Timecode, master: Option<BeatInfo>, bands: &[Option<BeatInfo>]) {
        self.0.push(Event { time: time.into(), data: EventData::Beat(master.is_some(), bands.iter().map(Option::is_some).collect()) });
    }

    fn window(&mut self, time: Timecode, info: WindowInfo) {}
}

fn main() {
    let mut args = std::env::args_os();

    if args.len() < 2 {
        eprintln!("Expected a filename");
        return;
    }

    let file = BufReader::new(File::open(args.nth(1).unwrap()).expect("open file failed"));
    let src = rodio::Decoder::new(file).expect("decode failed").buffered();

    println!("Running analysis...");

    let mut listener = DebugListener(Vec::new());
    analysis::Analyzer::new().run(src.clone(), &mut listener);

    println!("Last event is at time {} (source reports {:?})", dur_to_frac(listener.0.last().unwrap().time), src.total_duration().map(dur_to_frac));
    println!("Playing back...");

    let dev = rodio::default_output_device().expect("open default audio device filed");
    rodio::play_raw(&dev, src.convert_samples());

    let start = Instant::now();
    for ev in listener.0 {
        if let Some(dur) = ev.time.checked_sub(start.elapsed()) {
            std::thread::sleep(dur);
        }
        print!("{:>4.3}: ", dur_to_frac(ev.time));
        ev.data.pretty_print();
    }
}
