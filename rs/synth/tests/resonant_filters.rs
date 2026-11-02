//! Regression coverage for chaosprint/glicol#195 and pull request #196.
//! Exercise the raw DSP nodes so the language loader cannot mask filter failures.
use glicol_synth::{
    filter::{ResonantHighPassFilter, ResonantLowPassFilter},
    oscillator::SawOsc,
    signal::{ConstSig, Impulse},
    AudioContextBuilder, Node,
};

fn render<const N: usize>(
    source: impl Node<N> + Send + 'static,
    filter: impl Node<N> + Send + 'static,
    sample_rate: usize,
    cutoff_signal: Option<f32>,
) -> Vec<f32> {
    let mut context = AudioContextBuilder::<N>::new()
        .sr(sample_rate)
        .channels(1)
        .build();
    let source = context.add_mono_node(source);
    let filter = context.add_mono_node(filter);
    context.chain(vec![source, filter, context.destination]);
    if let Some(cutoff) = cutoff_signal {
        let control = context.add_mono_node(ConstSig::new(cutoff));
        context.connect(control, filter);
    }
    let mut output = Vec::with_capacity(sample_rate + N);
    while output.len() < sample_rate {
        output.extend_from_slice(&context.next_block()[0]);
    }
    output.truncate(sample_rate);
    assert!(output.iter().all(|sample| sample.is_finite()));
    output
}

fn peak(samples: &[f32]) -> f32 {
    samples
        .iter()
        .map(|sample| sample.abs())
        .fold(0.0, f32::max)
}

#[test]
fn highpass_rejects_dc_at_low_cutoff() {
    for sample_rate in [16000, 44100, 48000] {
        for control in [None, Some(90.0)] {
            let output = render::<128>(
                ConstSig::new(1.0),
                ResonantHighPassFilter::new()
                    .sr(sample_rate)
                    .cutoff(90.0)
                    .q(1.0),
                sample_rate,
                control,
            );
            let tail = &output[sample_rate * 9 / 10..];
            assert!(
                peak(tail) < 1e-3,
                "DC leaks: {} at {sample_rate} Hz, control={control:?}",
                peak(tail)
            );
        }
    }
}

#[test]
fn highpass_stays_bounded_on_a_65_hz_saw() {
    for sample_rate in [16000, 44100, 48000] {
        for control in [None, Some(90.0)] {
            let output = render::<128>(
                SawOsc::new().sr(sample_rate).freq(65.0),
                ResonantHighPassFilter::new()
                    .sr(sample_rate)
                    .cutoff(90.0)
                    .q(1.0),
                sample_rate,
                control,
            );
            assert!(
                peak(&output) < 2.0,
                "saw peak {} at {sample_rate} Hz, control={control:?}",
                peak(&output)
            );
        }
    }
}

#[test]
fn lowpass_passes_dc_at_unity() {
    for sample_rate in [16000, 44100, 48000] {
        for control in [None, Some(1000.0)] {
            let output = render::<128>(
                ConstSig::new(1.0),
                ResonantLowPassFilter::new()
                    .sr(sample_rate)
                    .cutoff(1000.0)
                    .q(1.0),
                sample_rate,
                control,
            );
            assert!(
                output[sample_rate * 9 / 10..]
                    .iter()
                    .all(|sample| (sample - 1.0).abs() < 1e-3),
                "incorrect lowpass DC gain at {sample_rate} Hz, control={control:?}"
            );
        }
    }
}

#[test]
fn both_filters_include_the_current_input_in_the_first_output_sample() {
    let sample_rate = 44100;
    let cutoff = 1000.0;
    let theta = std::f32::consts::TAU * cutoff / sample_rate as f32;
    let beta = 0.5 * (1.0 - theta.sin() / 2.0) / (1.0 + theta.sin() / 2.0);
    let gamma = (0.5 + beta) * theta.cos();
    for control in [None, Some(cutoff)] {
        let highpass = render::<1>(
            Impulse::new().sr(sample_rate).freq(1.0),
            ResonantHighPassFilter::new()
                .sr(sample_rate)
                .cutoff(cutoff)
                .q(1.0),
            sample_rate,
            control,
        );
        let lowpass = render::<1>(
            Impulse::new().sr(sample_rate).freq(1.0),
            ResonantLowPassFilter::new()
                .sr(sample_rate)
                .cutoff(cutoff)
                .q(1.0),
            sample_rate,
            control,
        );
        assert!((highpass[0] - (0.5 + beta + gamma) / 2.0).abs() < 1e-6);
        assert!((lowpass[0] - (0.5 + beta - gamma) / 2.0).abs() < 1e-6);
    }
}
