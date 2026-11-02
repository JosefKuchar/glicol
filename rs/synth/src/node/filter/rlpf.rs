use crate::{Buffer, Input, Message, Node};
use hashbrown::HashMap;
#[derive(Debug, Clone)]
pub struct ResonantLowPassFilter {
    // events: Vec<(f32, f32)>,
    pattern: Vec<(f32, f32)>,
    span: f32,
    bpm: f32,
    sr: usize,
    cutoff: f32,
    q: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
    step: usize,
    input_order: Vec<usize>,
}

impl Default for ResonantLowPassFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl ResonantLowPassFilter {
    pub fn new() -> Self {
        Self {
            // events: vec![],
            pattern: vec![],
            span: 1.,
            bpm: 120.,
            sr: 44100,
            step: 0,
            cutoff: 20.,
            q: 1.0,
            x1: 0.,
            x2: 0.,
            y1: 0.,
            y2: 0.,
            input_order: vec![],
        }
    }

    // pub fn events(self, events: Vec<(f32, f32)>) -> Self {
    //     Self {events, ..self}
    // }

    pub fn pattern(self, pattern: Vec<(f32, f32)>) -> Self {
        Self { pattern, ..self }
    }

    pub fn span(self, span: f32) -> Self {
        Self { span, ..self }
    }

    pub fn bpm(self, bpm: f32) -> Self {
        Self { bpm, ..self }
    }

    pub fn sr(self, sr: usize) -> Self {
        Self { sr, ..self }
    }

    pub fn cutoff(self, cutoff: f32) -> Self {
        Self { cutoff, ..self }
    }

    pub fn q(self, q: f32) -> Self {
        Self { q, ..self }
    }
}

impl<const N: usize> Node<N> for ResonantLowPassFilter {
    fn process(&mut self, inputs: &mut HashMap<usize, Input<N>>, output: &mut [Buffer<N>]) {
        // println!("inputs {:?} self.input_order {:?}", inputs, self.input_order);
        if inputs.is_empty() {
            return;
        }
        let main = inputs
            .get(self.input_order.first().unwrap_or(&usize::MAX))
            .or_else(|| inputs.values().next())
            .unwrap()
            .buffers();
        let control = self
            .input_order
            .get(1)
            .and_then(|id| inputs.get(id))
            .map(|i| i.buffers());
        for i in 0..N {
            let bar_dur = (240.0 / self.bpm * self.span * self.sr as f32).max(1.0) as usize;
            for &(value, time) in &self.pattern {
                if self.step % bar_dur == (time * 240.0 / self.bpm * self.sr as f32) as usize {
                    self.cutoff = value;
                }
            }
            let cutoff = control.map_or(self.cutoff, |b| b[0][i]);
            let cutoff = if cutoff.is_finite() {
                cutoff.clamp(0.0, self.sr as f32 * 0.499)
            } else {
                0.0
            };
            let q = if self.q.is_finite() {
                self.q.clamp(0.01, 100.0)
            } else {
                1.0
            };
            let theta = 2.0 * std::f32::consts::PI * cutoff / self.sr.max(1) as f32;
            let beta = 0.5 * (1.0 - theta.sin() / (2.0 * q)) / (1.0 + theta.sin() / (2.0 * q));
            let gamma = (0.5 + beta) * theta.cos();
            let a0 = (0.5 + beta - gamma) / 2.0;
            let a1 = 2.0 * a0;
            let x = main[0][i];
            let y =
                a0 * x + a1 * self.x1 + a0 * self.x2 + 2.0 * gamma * self.y1 - 2.0 * beta * self.y2;
            self.x2 = self.x1;
            self.x1 = x;
            self.y2 = self.y1;
            self.y1 = y;
            for channel in output.iter_mut() {
                channel[i] = y;
            }
            self.step += 1;
        }
    }

    fn send_msg(&mut self, info: Message) {
        match info {
            Message::SetPattern(p, span) => {
                self.pattern = p;
                self.span = span;
            }
            Message::SetToNumber(pos, value) => match pos {
                0 => self.cutoff = value,
                1 => self.q = value,
                _ => {}
            },
            Message::Index(i) => {
                // println!("got index without order {}", i);
                self.input_order.push(i)
            }
            Message::IndexOrder(pos, index) => {
                // println!("got index order {}", index);
                self.input_order.insert(pos, index)
            }
            Message::ResetOrder => {
                self.input_order.clear();
            }
            _ => {}
        }
    }
}
