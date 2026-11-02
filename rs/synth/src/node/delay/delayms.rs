use crate::{Buffer, Input, Message, Node};
use dasp_ring_buffer as ring_buffer;
type Fixed = ring_buffer::Fixed<Vec<f32>>;
use hashbrown::HashMap;
#[derive(Debug, Clone)]
pub struct DelayMs {
    buf: Vec<Fixed>,
    sr: usize,
    input_order: Vec<usize>,
    delay_n: usize,
}

impl Default for DelayMs {
    fn default() -> Self {
        Self::new()
    }
}

impl DelayMs {
    pub fn new() -> Self {
        Self {
            buf: vec![],
            delay_n: 1,
            sr: 44100,
            input_order: vec![],
        }
    }

    pub fn delay(self, delay: f32, chan: u8) -> Self {
        let delay_n = (delay / 1000. * self.sr as f32) as usize;
        let buf = vec![Fixed::from(vec![0.0; delay_n.max(1)]); chan as usize];

        Self {
            buf,
            delay_n,
            ..self
        }
    }

    pub fn sr(self, sr: usize) -> Self {
        Self { sr, ..self }
    }
}

impl<const N: usize> Node<N> for DelayMs {
    fn process(&mut self, inputs: &mut HashMap<usize, Input<N>>, output: &mut [Buffer<N>]) {
        let main_input = inputs.values().next().unwrap();
        if !(1..=2).contains(&main_input.buffers().len()) {
            return;
        }

        let output_channels = output.len();
        match inputs.len() {
            1 => {
                // no modulation
                match self.delay_n {
                    // equal to a pass node
                    0 => {
                        for (c, out) in output.iter_mut().enumerate() {
                            out.copy_from_slice(
                                &main_input.buffers()[c.min(main_input.buffers().len() - 1)],
                            );
                        }
                    }
                    _ => {
                        let iter = self.buf.iter_mut().zip(output.iter_mut()).zip(
                            (0..output_channels).map(|c| {
                                &main_input.buffers()[c.min(main_input.buffers().len() - 1)]
                            }),
                        );

                        for ((fixed, out_buf), main_buf) in iter {
                            for (out, main) in out_buf.iter_mut().zip(main_buf.iter()) {
                                *out = fixed.push(*main);
                            }
                        }
                    }
                }
            }
            2 => {
                let main_input = &inputs[&self.input_order[0]]; // can panic if there is no id
                let ref_input = &inputs[&self.input_order[1]]; // can panic if there is no id

                let mod_buf = &mut ref_input.buffers();
                for i in 0..N {
                    let delay = if mod_buf[0][i].is_finite() {
                        mod_buf[0][i].clamp(
                            1.0 / self.sr.max(1) as f32 * 1000.0,
                            self.buf[0].len() as f32 / self.sr.max(1) as f32 * 1000.0,
                        )
                    } else {
                        1.0
                    };
                    let pos =
                        (-delay / 1000. * self.sr as f32).rem_euclid(self.buf[0].len() as f32);
                    let pos_int = pos.floor() as usize;
                    let pos_frac = pos.fract();

                    let iter = self.buf.iter_mut().zip(output.iter_mut()).zip(
                        (0..output_channels)
                            .map(|c| &main_input.buffers()[c.min(main_input.buffers().len() - 1)]),
                    );

                    for ((fixed, out_buf), main_buf) in iter {
                        out_buf[i] = fixed.get(pos_int) * (1. - pos_frac)
                            + fixed.get((pos_int + 1) % fixed.len()) * pos_frac;
                        fixed.push(main_buf[i]);
                    }
                }

                // output[1][i] = self.buf2.get(pos_int) * pos_frac + self.buf2.get(pos_int+1) * (1.-pos_frac);

                // self.buf2.push(main_input.buffers()[1][i]);
            }
            _ => (),
        }
    }

    fn send_msg(&mut self, info: Message) {
        match info {
            Message::SetToNumber(0, value) => {
                let delay_n = (value / 1000. * self.sr as f32) as usize;
                self.delay_n = delay_n;

                if delay_n == 0 {
                    self.buf = vec![Fixed::from(vec![0.0]); self.buf.len()];
                } else {
                    let chan = self.buf.len();
                    self.buf = vec![Fixed::from(vec![0.0; delay_n]); chan];
                };
            }
            Message::Index(i) => self.input_order.push(i),
            Message::IndexOrder(pos, index) => self.input_order.insert(pos, index),
            Message::ResetOrder => {
                self.input_order.clear();
            }
            _ => {}
        }
    }
}
