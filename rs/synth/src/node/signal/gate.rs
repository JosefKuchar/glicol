use crate::{Buffer, Input, Message, Node};
use hashbrown::HashMap;
/// One finite gate, using absolute sample offsets within the node's lifetime.
pub struct Gate {
    on: usize,
    off: usize,
    step: usize,
}
impl Gate {
    pub fn new(on: usize, duration: usize) -> Self {
        Self {
            on,
            off: on.saturating_add(duration),
            step: 0,
        }
    }
}
impl<const N: usize> Node<N> for Gate {
    fn process(&mut self, _: &mut HashMap<usize, Input<N>>, out: &mut [Buffer<N>]) {
        for i in 0..N {
            let value = if self.step >= self.on && self.step < self.off {
                1.
            } else {
                0.
            };
            for channel in out.iter_mut() {
                channel[i] = value;
            }
            self.step = self.step.saturating_add(1);
        }
    }
    fn send_msg(&mut self, _: Message) {}
}
