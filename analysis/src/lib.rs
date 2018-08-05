extern crate rodio;
extern crate cpal;
extern crate rustfft;
extern crate itertools;

use std::time::Duration;
use std::convert::Into;

use cpal::Sample;
pub use rustfft::num_complex::Complex;
use itertools::Itertools;

#[derive(Clone,Copy,Debug)]
pub struct Timecode {
    pub sample: usize,
    pub sample_rate: usize,
}

impl Into<Duration> for Timecode {
    fn into(self) -> Duration {
        Duration::new(
            (self.sample / self.sample_rate) as u64,
            (1_000_000_000usize * (self.sample % self.sample_rate) / self.sample_rate) as u32
        )
    }
}

#[derive(Debug)]
pub struct Analyzer {
    win_size: usize,
    bands: usize,
    beat_e_factor: f64,
    rolling_e_mu: f64,
}

impl Default for Analyzer {
    fn default() -> Analyzer {
        Analyzer {
            win_size: 1024,
            bands: 8,
            beat_e_factor: 1.4,
            rolling_e_mu: 0.9,
        }
    }
}

#[derive(Debug,Clone)]
pub enum BeatInfo {
    EnergyBeat {
        rolling_e: f64,
        this_e: f64,
    },
}

#[derive(Debug,Clone)]
pub struct WindowInfo {
    energy: f64,
}

pub trait AnalysisListener {
    fn start(&mut self, _rate: usize, _bands: usize, _window: usize) {}
    fn stop(&mut self, _time: Timecode) {}

    fn beat(&mut self, _time: Timecode, _window_info: Option<BeatInfo>, _band_info: &[Option<BeatInfo>]) {}

    fn window(&mut self, _time: Timecode, _info: WindowInfo, _spectrum: &[Complex<f32>]) {}
}

impl Analyzer {
    pub fn new() -> Analyzer { Default::default() }

    pub fn with_windows(self, win_size: usize) -> Analyzer {
        Analyzer { win_size: win_size, ..self }
    }

    pub fn with_bands(self, bands: usize) -> Analyzer {
        Analyzer { bands: bands, ..self }
    }

    pub fn with_e_factor(self, e_factor: f64) -> Analyzer {
        Analyzer { beat_e_factor: e_factor, ..self }
    }

    pub fn with_e_mu(self, e_mu: f64) -> Analyzer {
        Analyzer { rolling_e_mu: e_mu, ..self }
    }

    pub fn run<S, L>(&mut self, source: S, listener: &mut L) 
    where S: rodio::Source, <S as Iterator>::Item: rodio::Sample, L: AnalysisListener {
        let rate = source.sample_rate() as usize;
        let channels = source.channels() as usize;
        let mut rolling_e = 0.0;

        let fft = rustfft::FFTplanner::new(false).plan_fft(self.win_size);
        let mut spectrum = std::iter::repeat(Complex::default()).take(self.win_size).collect::<Vec<_>>();

        // The `2` here is because the real DFT is (conjugate) symmetric around its midpoint.
        let slice_size = self.win_size / (2 * self.bands);
        let mut band_es = std::iter::repeat(0.0f64).take(self.bands).collect::<Vec<_>>();

        let mut band_states: Vec<Option<BeatInfo>> = std::iter::repeat(None).take(self.bands).collect();

        listener.start(rate, self.bands, self.win_size);

        let mut sample_counter = 0usize;

        for (idx, chunk_iter) in source.into_iter().step_by(channels).chunks(self.win_size).into_iter().enumerate() {
            let tc = Timecode { sample: idx * self.win_size, sample_rate: rate };

            let mut samples = chunk_iter.map(|s| Complex { re: s.to_f32(), im: 0.0 }).collect::<Vec<_>>();
            sample_counter += samples.len();
            if samples.len() != self.win_size { break; }
    
            let energy = samples.iter().map(|x| (x.re as f64) * (x.re as f64)).sum();
            let mut main_beat: Option<BeatInfo> = None;
            if energy > self.beat_e_factor * rolling_e {
                main_beat = Some(BeatInfo::EnergyBeat {
                    rolling_e: rolling_e,
                    this_e: energy,
                });
                rolling_e = energy;
            } else {
                rolling_e = self.rolling_e_mu * rolling_e + (1.0 - self.rolling_e_mu) * energy;
            }

            fft.process(&mut samples, &mut spectrum);
            for slidx in 0..self.bands {
                let low = slidx * slice_size;
                let high = low + slice_size;

                let band_e = spectrum[low..high].iter().map(|x| (x.re as f64) * (x.re as f64)).sum();
                if band_e > self.beat_e_factor * band_es[slidx] {
                    band_states[slidx] = Some(BeatInfo::EnergyBeat {
                        rolling_e: band_es[slidx],
                        this_e: band_e,
                    });
                    band_es[slidx] = band_e;
                } else {
                    band_states[slidx] = None;
                    band_es[slidx] = self.rolling_e_mu * band_es[slidx] + (1.0 - self.rolling_e_mu) * band_e;
                }
            }

            if main_beat.is_some() || band_states.iter().any(Option::is_some) {
                listener.beat(tc, main_beat, &band_states);
            }

            listener.window(tc, WindowInfo { energy: energy }, &spectrum);
        }

        listener.stop(Timecode { sample: sample_counter, sample_rate: rate});
    }
}
