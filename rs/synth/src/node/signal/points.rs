use crate::GlicolPara;
use crate::{Buffer, Input, Message, Node};
use glicol_parser::nodes::{Duration, TimeList};
use hashbrown::HashMap;

#[derive(Debug, Clone)]
pub struct Points {
    pub point_list: Vec<(usize, f32)>, // time and value
    pub span: f32,
    pub bpm: f32,
    pub sr: usize,
    step: usize,
    _index: usize,
    is_looping: bool,
    input_order: Vec<usize>,
}

impl Default for Points {
    fn default() -> Self {
        Self::new()
    }
}

impl Points {
    pub fn new() -> Self {
        // points: GlicolPara, bpm: f32, sr: usize, span: f32
        // let mut point_list = process_points(points, bpm, sr, span);
        // println!("point_list {:?}", point_list);
        Self {
            point_list: vec![],
            span: 1.0,
            bpm: 120.,
            sr: 44100,
            step: 0,
            _index: 0,
            is_looping: false,
            input_order: vec![],
        }
    }

    pub fn span(self, span: f32) -> Self {
        Self { span, ..self }
    }

    pub fn is_looping(self, is_looping: bool) -> Self {
        Self { is_looping, ..self }
    }

    pub fn bpm(self, bpm: f32) -> Self {
        Self { bpm, ..self }
    }

    pub fn sr(self, sr: usize) -> Self {
        Self { sr, ..self }
    }

    pub fn points(self, points: Vec<(TimeList, f32)>) -> Self {
        let bpm = self.bpm;
        let sr = self.sr;
        let span = self.span;
        let point_list = self.make_point_list(points, bpm, sr, span);
        Self { point_list, ..self }
    }

    fn make_point_list(
        &self,
        points: Vec<(TimeList, f32)>,
        bpm: f32,
        sr: usize,
        span: f32,
    ) -> Vec<(usize, f32)> {
        let mut point_list = points
            .into_iter()
            .map(|(time, value)| {
                let cycle_dur = 60. / bpm * 4.;
                let bar_dur = cycle_dur * span * sr as f32;

                let bar_pos = time.bar * bar_dur;
                let time_pos = time.time.map_or(0., |kind| match kind {
                    Duration::Bar(b) => b * bar_dur,
                    Duration::Seconds(s) => s * (sr as f32),
                    Duration::Milliseconds(ms) => (ms / 1000.0) * sr as f32,
                });

                ((bar_pos + time_pos) as usize, value)
            })
            .collect::<Vec<_>>();

        point_list.sort_by_key(|p| p.0);
        // Last value wins when timestamps quantize to the same sample.
        let mut normalized: Vec<(usize, f32)> = Vec::new();
        for point in point_list {
            if normalized.last().is_some_and(|p| p.0 == point.0) {
                normalized.pop();
            }
            normalized.push(point);
        }
        let mut point_list = normalized;
        if point_list.first().is_some_and(|p| p.0 != 0) {
            point_list.insert(0, (0, 0.0));
        }

        point_list
    }
}

impl<const N: usize> Node<N> for Points {
    fn process(&mut self, _inputs: &mut HashMap<usize, Input<N>>, output: &mut [Buffer<N>]) {
        let period = (240.0 / self.bpm * self.span * self.sr as f32).max(1.0) as usize;
        for i in 0..N {
            let pos = if self.is_looping {
                self.step % period
            } else {
                self.step
            };
            let end = self.point_list.partition_point(|p| p.0 <= pos);
            let value = if self.point_list.is_empty() {
                0.0
            } else if end == 0 {
                self.point_list[0].1
            } else if end < self.point_list.len() {
                let (a, x) = self.point_list[end - 1];
                let (b, y) = self.point_list[end];
                x + (y - x) * (pos - a) as f32 / (b - a) as f32
            } else {
                let (a, x) = self.point_list[end - 1];
                if self.is_looping && a < period {
                    x + (self.point_list[0].1 - x) * (pos - a) as f32 / (period - a) as f32
                } else {
                    x
                }
            };
            for channel in output.iter_mut() {
                channel[i] = value;
            }
            self.step += 1;
        }
    }

    fn send_msg(&mut self, info: Message) {
        match info {
            Message::SetParam(0, GlicolPara::Points(time_list)) => {
                self.point_list = self.make_point_list(time_list, self.bpm, self.sr, self.span);
                self.step = 0;
            }
            Message::SetToNumber(1, num) => {
                self.span = num;
                self.step = 0;
            }
            Message::SetToBool(2, b) => {
                self.is_looping = b;
                self.step = 0;
            }
            Message::SetBPM(bpm) => self.bpm = bpm,
            Message::Index(i) => self.input_order.push(i),
            Message::IndexOrder(pos, index) => self.input_order.insert(pos, index),
            _ => {}
        }
    }
}
