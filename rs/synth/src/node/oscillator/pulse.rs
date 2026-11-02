use crate::{Buffer, Input, Message, Node};
use hashbrown::HashMap;

/// Bandlimited pulse with sample-wise frequency and width controls.
/// Width is a fraction of a cycle; frequency is in Hz.
pub struct PulseOsc {
    freq: f32,
    width: f32,
    phase: f32,
    sr: usize,
    freq_ref: Option<usize>,
    width_ref: Option<usize>,
    order: Vec<usize>,
}
impl PulseOsc {
    pub fn new(freq: f32, width: f32, sr: usize) -> Self {
        Self {
            freq,
            width,
            phase: 0.,
            sr: sr.max(1),
            freq_ref: None,
            width_ref: None,
            order: vec![],
        }
    }
    pub fn controls(mut self, freq: Option<usize>, width: Option<usize>) -> Self {
        self.freq_ref = freq;
        self.width_ref = width;
        self
    }
}
impl<const N: usize> Node<N> for PulseOsc {
    fn process(&mut self, inputs: &mut HashMap<usize, Input<N>>, out: &mut [Buffer<N>]) {
        for i in 0..N {
            let control = |slot: Option<usize>, fallback: f32| {
                slot.and_then(|s| self.order.get(s))
                    .and_then(|id| inputs.get(id))
                    .map_or(fallback, |b| b.buffers()[0][i])
            };
            let step = super::phase_step(control(self.freq_ref, self.freq), self.sr);
            let width = control(self.width_ref, self.width);
            let width = if width.is_finite() {
                width.clamp(step.abs().max(0.001), 1.0 - step.abs().max(0.001))
            } else {
                0.5
            };
            let mut value = if self.phase < width { 1.0 } else { -1.0 };
            value += super::poly_blep(self.phase, step.abs())
                - super::poly_blep((self.phase - width).rem_euclid(1.0), step.abs());
            for channel in out.iter_mut() {
                channel[i] = value;
            }
            self.phase = (self.phase + step).rem_euclid(1.0);
        }
    }
    fn send_msg(&mut self, msg: Message) {
        match msg {
            Message::SetToNumber(0, v) => self.freq = v,
            Message::SetToNumber(1, v) => self.width = v,
            Message::Index(id) => self.order.push(id),
            Message::IndexOrder(p, id) => self.order.insert(p.min(self.order.len()), id),
            Message::ResetOrder => self.order.clear(),
            _ => {}
        }
    }
}
