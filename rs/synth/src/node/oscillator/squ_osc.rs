use crate::{oscillator::process_oscillation, Buffer, Input, Message, Node};
use hashbrown::HashMap;
#[derive(Debug, Clone)]
pub struct SquOsc {
    pub freq: f32,
    pub phase: f32,
    pub sr: usize,
    inc: f32,
    input_order: Vec<usize>,
}

impl std::default::Default for SquOsc {
    fn default() -> Self {
        Self {
            freq: 1.0,
            phase: 0.0,
            sr: 44100,
            inc: 0.,
            input_order: vec![],
        }
    }
}

impl SquOsc {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn freq(self, freq: f32) -> Self {
        Self { freq, ..self }
    }
    pub fn sr(self, sr: usize) -> Self {
        Self { sr, ..self }
    }
    pub fn phase(self, phase: f32) -> Self {
        Self { phase, ..self }
    }
}

impl<const N: usize> Node<N> for SquOsc {
    fn process(&mut self, inputs: &mut HashMap<usize, Input<N>>, output: &mut [Buffer<N>]) {
        process_oscillation(
            inputs,
            &mut self.input_order,
            output,
            self.freq,
            &mut self.inc,
            |out, freq| {
                let step = super::phase_step(freq, self.sr);
                self.phase = self.phase.rem_euclid(1.0);
                *out = if self.phase < 0.5 { 1.0 } else { -1.0 };
                *out += super::poly_blep(self.phase, step.abs())
                    - super::poly_blep((self.phase + 0.5).rem_euclid(1.0), step.abs());
                self.phase = (self.phase + step).rem_euclid(1.0);
            },
        );
    }

    fn send_msg(&mut self, info: Message) {
        match info {
            Message::SetToNumber(0, value) => self.freq = value,
            Message::Index(i) => self.input_order.push(i),
            Message::IndexOrder(pos, index) => self.input_order.insert(pos, index),
            Message::ResetOrder => {
                self.input_order.clear();
            }
            _ => {}
        }
    }
}
