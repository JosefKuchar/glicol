pub use sin_osc::SinOsc;
mod sin_osc;
pub use saw_osc::SawOsc;
mod saw_osc;
pub use tri_osc::TriOsc;
mod tri_osc;
pub use squ_osc::SquOsc;
mod squ_osc;

use crate::{Buffer, Input};
use hashbrown::HashMap;

fn process_oscillation<const N: usize>(
    inputs: &mut HashMap<usize, Input<N>>,
    input_order: &mut [usize],
    output: &mut [Buffer<N>],
    freq: f32,
    inc: &mut f32,
    mut osc: impl FnMut(&mut f32, f32),
) {
    match inputs.len() {
        0 => {
            for out in &mut *output[0] {
                osc(out, freq);
            }
        }
        1 => {
            let mod_input = match input_order {
                [] => &mut *inputs.values_mut().next().unwrap(),
                [ref first_input, ..] => &inputs[first_input],
            };

            for (out, mod_buf) in output[0].iter_mut().zip(mod_input.buffers()[0].iter()) {
                if *mod_buf != 0. {
                    *inc = *mod_buf;
                };

                osc(out, *inc);
            }
        }
        _ => {}
    }
}

/// Polynomial BLEP correction for a unit-phase discontinuity.
pub(crate) fn poly_blep(t: f32, dt: f32) -> f32 {
    if dt <= 0.0 {
        return 0.0;
    }
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}
pub(crate) fn phase_step(freq: f32, sr: usize) -> f32 {
    if freq.is_finite() {
        (freq / sr.max(1) as f32).clamp(-0.499, 0.499)
    } else {
        0.0
    }
}
mod pulse;
pub use pulse::PulseOsc;
mod wavetable;
pub use wavetable::{WavetableBank, WavetableOsc};
