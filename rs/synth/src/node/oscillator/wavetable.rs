use crate::{Buffer, Input, Message, Node};
use hashbrown::HashMap;
use std::sync::Arc;

/// Immutable periodic frames, with harmonic-limited mip levels.
/// Construction runs offline; processing allocates no tables.
pub struct WavetableBank {
    frames: Vec<Vec<Vec<f32>>>,
    len: usize,
}
impl WavetableBank {
    /// Harmonic sine amplitudes: index 0 is the fundamental, index 1 the second harmonic.
    pub fn from_harmonics(frames: &[Vec<f32>], len: usize) -> Result<Self, String> {
        if len < 32
            || len > 2048
            || !len.is_power_of_two()
            || frames.is_empty()
            || frames.len() > 256
        {
            return Err("wavetable needs 1..256 frames and a power-of-two size in 32..2048".into());
        }
        if frames
            .iter()
            .any(|f| f.is_empty() || f.len() > len / 2 || f.iter().any(|v| !v.is_finite()))
        {
            return Err("invalid harmonic amplitudes".into());
        }
        let mut tables = Vec::new();
        for frame in frames {
            let mut levels = Vec::new();
            let mut cap = len / 2;
            loop {
                let table = (0..len)
                    .map(|i| {
                        frame
                            .iter()
                            .take(cap)
                            .enumerate()
                            .map(|(h, a)| {
                                a * (std::f32::consts::TAU * (h + 1) as f32 * i as f32 / len as f32)
                                    .sin()
                            })
                            .sum()
                    })
                    .collect();
                levels.push(table);
                if cap == 1 {
                    break;
                }
                cap /= 2;
            }
            tables.push(levels);
        }
        Ok(Self {
            frames: tables,
            len,
        })
    }
    /// Arbitrary single-cycle samples. DFT and mip construction happen at registration.
    pub fn from_frames(frames: &[Vec<f32>]) -> Result<Self, String> {
        let len = frames.first().map_or(0, Vec::len);
        if len < 32
            || len > 2048
            || !len.is_power_of_two()
            || frames.is_empty()
            || frames.len() > 256
            || frames
                .iter()
                .any(|f| f.len() != len || f.iter().any(|v| !v.is_finite()))
        {
            return Err("wavetable frames must have equal power-of-two lengths in 32..2048 and finite samples".into());
        }
        let mut tables = Vec::new();
        for frame in frames {
            let dc = frame.iter().sum::<f32>() / len as f32;
            let coeffs = (1..len / 2)
                .map(|h| {
                    let mut sin = 0.;
                    let mut cos = 0.;
                    for (i, x) in frame.iter().enumerate() {
                        let phase = std::f32::consts::TAU * h as f32 * i as f32 / len as f32;
                        sin += x * phase.sin();
                        cos += x * phase.cos();
                    }
                    (sin * 2. / len as f32, cos * 2. / len as f32)
                })
                .collect::<Vec<_>>();
            let mut levels = Vec::new();
            let mut cap = len / 2;
            loop {
                levels.push(
                    (0..len)
                        .map(|i| {
                            dc + coeffs
                                .iter()
                                .take(cap)
                                .enumerate()
                                .map(|(h, (s, c))| {
                                    let p = std::f32::consts::TAU * (h + 1) as f32 * i as f32
                                        / len as f32;
                                    s * p.sin() + c * p.cos()
                                })
                                .sum::<f32>()
                        })
                        .collect(),
                );
                if cap == 1 {
                    break;
                }
                cap /= 2;
            }
            tables.push(levels);
        }
        Ok(Self {
            frames: tables,
            len,
        })
    }
    pub fn basic() -> Self {
        Self::from_harmonics(
            &[
                vec![1.],
                (1..=128)
                    .map(|h| -2.0 / (std::f32::consts::PI * h as f32))
                    .collect(),
                (1..=128)
                    .map(|h| {
                        if h % 2 == 1 {
                            4.0 / (std::f32::consts::PI * h as f32)
                        } else {
                            0.
                        }
                    })
                    .collect(),
            ],
            256,
        )
        .expect("valid built-in harmonics")
    }
    fn sample(&self, phase: f32, position: f32, freq: f32, sr: usize) -> f32 {
        let position = position.clamp(0., 1.) * (self.frames.len() - 1) as f32;
        let a = position.floor() as usize;
        let b = (a + 1).min(self.frames.len() - 1);
        let blend = position.fract();
        // An extra octave of margin permits a smooth crossfade between two safe mip levels.
        let mip = ((freq.abs() * (self.len / 2) as f32 / (sr as f32 * 0.45))
            .max(f32::MIN_POSITIVE)
            .log2()
            + 1.)
            .clamp(0., (self.frames[0].len() - 1) as f32);
        let m = mip.floor() as usize;
        let n = (m + 1).min(self.frames[0].len() - 1);
        let x = phase * self.len as f32;
        let i = x.floor() as usize % self.len;
        let j = (i + 1) % self.len;
        let read = |frame: usize, level: usize| {
            let t = &self.frames[frame][level];
            t[i] + (t[j] - t[i]) * x.fract()
        };
        let read_frame = |frame| {
            let u = read(frame, m);
            u + (read(frame, n) - u) * mip.fract()
        };
        let u = read_frame(a);
        u + (read_frame(b) - u) * blend
    }
}

pub struct WavetableOsc {
    bank: Arc<WavetableBank>,
    freq: f32,
    position: f32,
    phase: f32,
    sr: usize,
    freq_ref: Option<usize>,
    position_ref: Option<usize>,
    order: Vec<usize>,
}
impl WavetableOsc {
    pub fn new(bank: Arc<WavetableBank>, freq: f32, position: f32, sr: usize) -> Self {
        Self {
            bank,
            freq,
            position,
            phase: 0.,
            sr: sr.max(1),
            freq_ref: None,
            position_ref: None,
            order: vec![],
        }
    }
    pub fn controls(mut self, freq: Option<usize>, position: Option<usize>) -> Self {
        self.freq_ref = freq;
        self.position_ref = position;
        self
    }
}
impl<const N: usize> Node<N> for WavetableOsc {
    fn process(&mut self, inputs: &mut HashMap<usize, Input<N>>, out: &mut [Buffer<N>]) {
        for i in 0..N {
            let control = |slot: Option<usize>, fallback: f32| {
                slot.and_then(|s| self.order.get(s))
                    .and_then(|id| inputs.get(id))
                    .map_or(fallback, |b| b.buffers()[0][i])
            };
            let freq = control(self.freq_ref, self.freq);
            let position = control(self.position_ref, self.position);
            let step = super::phase_step(freq, self.sr);
            let value =
                if freq.is_finite() && position.is_finite() && freq.abs() < self.sr as f32 * 0.5 {
                    self.bank.sample(self.phase, position, freq, self.sr)
                } else {
                    0.
                };
            for channel in out.iter_mut() {
                channel[i] = value;
            }
            self.phase = (self.phase + step).rem_euclid(1.);
        }
    }
    fn send_msg(&mut self, msg: Message) {
        match msg {
            Message::SetToNumber(0, v) => self.freq = v,
            Message::SetToNumber(1, v) => self.position = v,
            Message::Index(id) => self.order.push(id),
            Message::IndexOrder(p, id) => self.order.insert(p.min(self.order.len()), id),
            Message::ResetOrder => self.order.clear(),
            _ => {}
        }
    }
}
