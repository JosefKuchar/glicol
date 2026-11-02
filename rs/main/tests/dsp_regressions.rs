use glicol::{Engine, EngineError, NoteEvent, OfflineRenderer};
use glicol_synth::oscillator::WavetableBank;
use std::sync::Arc;

fn render<const N: usize>(code: &str, sr: usize, len: usize) -> Vec<f32> {
    let mut engine = Engine::<N>::new();
    engine.try_set_sr(sr).unwrap();
    engine.update_with_code(code).unwrap();
    let mut out = Vec::new();
    while out.len() < len {
        out.extend_from_slice(&engine.try_next_block(vec![]).unwrap()[0]);
    }
    out.truncate(len);
    out
}
fn close(a: &[f32], b: &[f32], eps: f32) {
    assert_eq!(a.len(), b.len());
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        assert!((x - y).abs() <= eps, "sample {i}: {x} != {y}");
    }
}
#[test]
fn automation_holds_end_and_handles_exact_points() {
    let out = render::<8>("out: [0=>0 0+0.004_s=>1 0+0.008_s=>0.5]", 1000, 20);
    close(&out[..5], &[0., 0.25, 0.5, 0.75, 1.], 1e-6);
    assert_eq!(out[8], 0.5);
    assert_eq!(out[19], 0.5);
}
#[test]
fn automation_loops_without_nonfinite_samples() {
    let out = render::<64>("out: [0=>0 1/2=>1]!", 1000, 4001);
    assert_eq!(out[0], 0.);
    assert_eq!(out[1000], 1.);
    assert_eq!(out[2000], 0.);
    assert_eq!(out[3000], 1.);
    assert!(out.iter().all(|x| x.is_finite() && (0.0..=1.).contains(x)));
}
#[test]
fn coincident_points_use_last_value() {
    let out = render::<8>("out: [0=>0 0=>1 0+0.004_s=>0]", 1000, 12);
    assert_eq!(out[0], 1.);
    assert_eq!(out[4], 0.);
    assert_eq!(out[11], 0.);
}
#[test]
fn invalid_curves_and_parameters_are_errors() {
    for code in [
        "out: []",
        "out: [0=>0 1=>1]/0",
        "out: sig 1 >> adsr -1 0.1 0.5 0.1",
        "out: sin 440 >> lpf 1000 0",
        "out: gate -1 1",
        "out: pulse 440 2",
    ] {
        assert!(
            Engine::<64>::new().update_with_code(code).is_err(),
            "{code}"
        );
    }
}
#[test]
fn gate_timing_is_exact_and_nonlooping() {
    let out = render::<64>("out: gate 0.013 0.017", 1000, 128);
    assert!(out[..13].iter().all(|x| *x == 0.));
    assert!(out[13..30].iter().all(|x| *x == 1.));
    assert!(out[30..].iter().all(|x| *x == 0.));
}
#[test]
fn adsr_retrigger_starts_at_current_level() {
    let code =
        "~a: gate 0 0.02\n~b: gate 0.025 0.02\n~g: mix ~a ~b\nout: ~g >> adsr 0.01 0 0.6 0.02";
    let out = render::<64>(code, 1000, 70);
    assert!((out[25] - out[24]).abs() < 1e-6, "{} {}", out[24], out[25]);
    assert!(out[26] > out[25]);
    assert_eq!(out[69], 0.);
}
#[test]
fn zero_length_envelope_stages_are_defined() {
    let out = render::<8>("out: gate 0 0.004 >> adsr 0 0 0.5 0", 1000, 10);
    assert_eq!(out[0], 1.);
    assert_eq!(out[1], 0.5);
    assert_eq!(out[4], 0.);
    assert!(out.iter().all(|x| x.is_finite()));
}
#[test]
fn filters_modulate_sample_by_sample() {
    for kind in ["lpf", "hpf"] {
        let code = format!("~f: sin 731 >> mul 900 >> add 1500\nout: saw 110 >> {kind} ~f 0.7");
        close(
            &render::<1>(&code, 16000, 2048),
            &render::<64>(&code, 16000, 2048),
            1e-6,
        );
    }
}
#[test]
fn lowpass_passes_dc_and_highpass_rejects_it() {
    let low = render::<64>("out: sig 1 >> lpf 1000 0.7", 16000, 4096);
    let high = render::<64>("out: sig 1 >> hpf 1000 0.7", 16000, 4096);
    assert!((low[4095] - 1.).abs() < 1e-4);
    assert!(high[4095].abs() < 1e-4);
}
#[test]
fn compound_drums_have_sample_rate_correct_envelopes() {
    for voice in ["bd", "sn", "hh"] {
        for sr in [16000, 48000] {
            let code = format!("out: imp 0.2 >> {voice} 0.2");
            let out = render::<64>(&code, sr, sr);
            assert!(out.iter().any(|x| x.abs() > 1e-5));
            let last = out.iter().rposition(|x| x.abs() > 1e-8).unwrap();
            let attack = if voice == "sn" { 0.001 } else { 0.003 };
            let expected = ((0.2 + attack) * sr as f64) as usize;
            assert!(
                (last as isize - expected as isize).abs() < 64,
                "{voice}@{sr}: {last} expected {expected}"
            );
        }
    }
}
#[test]
fn stereo_delay_duplicates_mono_and_handles_zero_delay() {
    for ms in [0, 10] {
        let mut e = Engine::<64>::new();
        e.set_sr(16000);
        e.update_with_code(&format!("out: sig 1 >> delayms {ms}"))
            .unwrap();
        let mut values = Vec::new();
        for _ in 0..4 {
            let b = e.try_next_block(vec![]).unwrap();
            assert_eq!(b[0], b[1]);
            values.extend_from_slice(&b[0]);
        }
        assert_eq!(values.iter().position(|x| *x != 0.), Some(ms * 16));
    }
}
#[test]
fn fractional_delay_is_bounded_and_continuous() {
    let a = render::<1>("~d: sig 10.5\nout: sig 1 >> delayms ~d", 1000, 20);
    assert_eq!(a[9], 0.);
    assert_eq!(a[10], 0.5);
    assert_eq!(a[11], 1.);
}
#[test]
fn unsupported_nodes_and_invalid_scripts_return_errors() {
    for code in [
        "out: expr `sin(phase)`",
        "out: sin 440 >> reverb 0.1 0.2 0.3 0.4 0.5",
        "out: eval `unknown(1)`",
        "out: psynth `invalid`",
    ] {
        assert!(
            Engine::<64>::new().update_with_code(code).is_err(),
            "{code}"
        );
    }
}
#[test]
fn runtime_script_errors_and_nonfinite_output_stop_checked_render() {
    let mut e = Engine::<64>::new();
    e.update_with_code("out: eval `1/(phase-3)`").unwrap();
    assert!(e.try_next_block(vec![]).is_err());
    assert!(e.try_next_block(vec![]).is_err());
    e.update_with_code("out: sin 440").unwrap();
    assert!(e.try_next_block(vec![]).is_ok());
}
#[cfg(feature = "use-meta")]
#[test]
fn rhai_compile_runtime_and_output_shape_errors_are_reported() {
    assert!(Engine::<64>::new()
        .update_with_code("out: meta `not valid rhai!`")
        .is_err());
    for script in ["unknown_function()", "[1.0]", "[true]"] {
        let mut e = Engine::<64>::new();
        e.update_with_code(&format!("out: meta `{script}`"))
            .unwrap();
        assert!(e.try_next_block(vec![]).is_err());
    }
}
#[test]
fn failed_update_does_not_return_old_audio_from_checked_api() {
    let mut e = Engine::<64>::new();
    e.update_with_code("out: sin 440").unwrap();
    e.try_next_block(vec![]).unwrap();
    assert!(e.update_with_code("out: sin ~missing").is_err());
    assert!(e.try_next_block(vec![]).is_err());
    e.update_with_code("out: sin 440").unwrap();
    assert!(e.try_next_block(vec![]).is_ok());
}
#[test]
fn malformed_messages_never_index_or_parse_panic() {
    let mut e = Engine::<64>::new();
    e.update_with_code("out: sin 440").unwrap();
    for msg in [
        "out,500,0,1",
        "missing,0,0,1",
        "out,no,0,1",
        "out,0,0,NaN",
        "bad",
    ] {
        assert!(e.try_send_msg(msg).is_err(), "{msg}");
    }
    e.send_msg("out,500,0,1");
    assert!(e.try_next_block(vec![]).is_err());
    let mut e = Engine::<64>::new();
    e.update_with_code("out: msgsynth \\saw 0.01 0.1").unwrap();
    assert!(e.try_send_msg("out,0,3,broken=>note").is_err());
}
#[test]
fn rate_changes_require_reset_and_input_lengths_are_checked() {
    let mut e = Engine::<64>::new();
    assert!(e.try_set_sr(0).is_err());
    e.set_sr(16000);
    e.update_with_code("out: sin 440").unwrap();
    assert!(e.try_set_sr(48000).is_err());
    assert!(e.try_next_block(vec![&[0.; 3]]).is_err());
    e.reset();
    e.try_set_sr(48000).unwrap();
    e.update_with_code("out: ~input").unwrap();
    assert!(e.try_next_block(vec![&[0.2; 64]]).is_ok());
}
#[test]
fn wavetable_morph_and_custom_registration_work() {
    let bank = Arc::new(WavetableBank::from_harmonics(&[vec![1.], vec![-1.]], 256).unwrap());
    let mut signals = Vec::new();
    for pos in [0., 0.5, 1.] {
        let mut e = Engine::<64>::new();
        e.set_sr(16000);
        e.add_wavetable("custom", bank.clone()).unwrap();
        e.update_with_code(&format!("out: wavetable \\custom 440 {pos:.1}"))
            .unwrap();
        signals.push(e.try_next_block(vec![]).unwrap()[0].to_vec());
    }
    assert!(signals[0].iter().any(|x| x.abs() > 0.5));
    assert!(signals[1].iter().all(|x| x.abs() < 1e-6));
    close(
        &signals[0],
        &signals[2].iter().map(|v| -v).collect::<Vec<_>>(),
        1e-6,
    );
    assert!(Engine::<64>::new()
        .update_with_code("out: wavetable \\missing 440 0")
        .is_err());
}
#[test]
fn arbitrary_sample_wavetables_preserve_phase_and_dc() {
    let n = 64;
    let frame = (0..n)
        .map(|i| 0.2 + 0.6 * (std::f32::consts::TAU * i as f32 / n as f32).cos())
        .collect::<Vec<_>>();
    let bank = Arc::new(WavetableBank::from_frames(&[frame]).unwrap());
    let mut e = Engine::<64>::new();
    e.add_wavetable("cosine", bank).unwrap();
    e.update_with_code("out: wavetable \\cosine 440 0").unwrap();
    assert!((e.try_next_block(vec![]).unwrap()[0][0] - 0.8).abs() < 1e-5);
}
#[test]
fn pulse_and_wavetable_controls_are_block_invariant() {
    for code in ["~f: sin 33 >> mul 100 >> add 440\n~w: sin 71 >> mul 0.2 >> add 0.5\nout: pulse ~f ~w","~f: sin 33 >> mul 100 >> add 440\n~w: sin 71 >> mul 0.2 >> add 0.5\nout: wavetable \\basic ~f ~w"]{
        close(&render::<1>(code,16000,2048),&render::<64>(code,16000,2048),1e-6);
    }
}
fn magnitude(samples: &[f32], freq: f32, sr: f32) -> f32 {
    let mut re = 0.;
    let mut im = 0.;
    for (i, x) in samples.iter().enumerate() {
        let p = std::f32::consts::TAU * freq * i as f32 / sr;
        re += x * p.cos();
        im += x * p.sin();
    }
    (re * re + im * im).sqrt() / samples.len() as f32
}
#[test]
fn bandlimited_saw_reduces_folded_second_harmonic() {
    let sr = 16000.;
    let freq = 5000.;
    let count = 4096;
    let out = render::<64>("out: saw 5000", sr as usize, count);
    let naive = (0..count)
        .map(|i| (i as f32 * freq / sr).fract() * 2. - 1.)
        .collect::<Vec<_>>();
    let folded = magnitude(&out, 6000., sr);
    let baseline = magnitude(&naive, 6000., sr);
    assert!(folded < baseline * 0.5, "alias {folded}, naive {baseline}");
    assert!(magnitude(&out, 5000., sr) > 0.1);
}
fn note(onset: usize, duration: usize, pitch: f32, velocity: f32) -> NoteEvent {
    NoteEvent {
        onset_seconds: onset as f64 / 16000.,
        duration_seconds: duration as f64 / 16000.,
        midi_pitch: pitch,
        velocity,
    }
}
#[test]
fn offline_notes_mix_exact_offsets_velocity_and_releases() {
    let renderer = OfflineRenderer::<64>::new(16000).unwrap();
    let patch =
        "~env: ~gate >> adsr 0.001 0.01 0.6 0.02\nout: sin ~pitch >> mul ~env >> mul ~velocity";
    let a = note(137, 1097, 57., 0.3);
    let b = note(711, 1290, 57., 0.15);
    let mixed = renderer.render_notes(patch, &[a, b], 3001, 0.02).unwrap();
    let av = renderer.render_notes(patch, &[a], 3001, 0.02).unwrap();
    let bv = renderer.render_notes(patch, &[b], 3001, 0.02).unwrap();
    assert_eq!(mixed.len(), 3001);
    for i in 0..mixed.len() {
        for c in 0..2 {
            assert!((mixed[i][c] - av[i][c] - bv[i][c]).abs() < 1e-6);
        }
    }
    assert_eq!(mixed.iter().position(|x| x[0].abs() > 1e-8), Some(138));
    assert_eq!(mixed.iter().rposition(|x| x[0].abs() > 1e-8), Some(2320));
    let loud = renderer
        .render_notes(patch, &[note(137, 1097, 57., 0.6)], 3001, 0.02)
        .unwrap();
    for i in 0..3001 {
        assert!((loud[i][0] - 2. * av[i][0]).abs() < 1e-6);
    }
}
#[test]
fn offline_renderer_validates_even_empty_performances() {
    let renderer = OfflineRenderer::<64>::new(16000).unwrap();
    assert!(renderer.render_notes("bad", &[], 100, 0.1).is_err());
    assert!(renderer
        .render_notes("~pitch: sig 1\nout: sin ~pitch", &[], 100, 0.1)
        .is_err());
    assert!(renderer
        .render_notes("out: sin ~pitch", &[note(0, 10, 200., 0.5)], 100, 0.1)
        .is_err());
    assert!(OfflineRenderer::<0>::new(16000).is_err());
}
#[test]
fn representative_patches_and_plate_are_finite_at_both_rates() {
    for sr in [16000, 48000] {
        for code in ["~e: imp 1 >> envperc 0.002 0.3\nout: tri 330 >> mul ~e","~f: sin 2 >> mul 500 >> add 1000\nout: saw 110 >> lpf ~f 0.7","~a: sin 0.3 >> mul 0.1 >> add 0.2\n~x: saw 219 >> mul ~a\n~y: saw 221 >> mul ~a\nout: mix ~x ~y >> lpf 1800 0.7 >> pan 0.2","out: imp 1 >> plate 0.5"]{
        let out=render::<64>(code,sr,sr+17);assert!(out.iter().all(|x|x.is_finite()));assert!(out.iter().any(|x|x.abs()>1e-6));
    }
    }
}
#[test]
fn seeded_noise_is_repeatable() {
    assert_eq!(
        render::<64>("out: noise 42", 16000, 1001),
        render::<64>("out: noise 42", 16000, 1001)
    );
}
#[test]
fn errors_keep_their_kind() {
    assert!(matches!(
        Engine::<64>::new().update_with_code("out: expr `1`"),
        Err(EngineError::UnsupportedNode(_))
    ));
}

#[test]
fn invalid_effect_topology_is_rejected_before_rendering() {
    for code in [
        "out: delayms 10",
        "out: plate 0.2",
        "out: adsr 0.1 0.1 0.5 0.2",
        "out: sig 1 >> pulse 440 0.5",
    ] {
        assert!(
            Engine::<64>::new().update_with_code(code).is_err(),
            "{code}"
        );
    }
}
#[test]
fn eval_parser_preserves_entire_expression_and_state() {
    let out = render::<8>("out: eval `x:=x+1;x`", 16000, 10);
    assert_eq!(out, (1..=10).map(|i| i as f32).collect::<Vec<_>>());
}
#[cfg(feature = "use-meta")]
#[test]
fn rhai_infinite_loop_returns_a_runtime_error() {
    let mut e = Engine::<64>::new();
    e.update_with_code("out: meta `loop {}`").unwrap();
    assert!(e.try_next_block(vec![]).is_err());
}
