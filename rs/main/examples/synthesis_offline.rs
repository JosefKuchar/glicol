//! cargo run --release -p glicol --example synthesis_offline -- /tmp/glicol-demo.wav
use glicol::{NoteEvent, OfflineRenderer};
use std::{
    fs::File,
    io::{BufWriter, Write},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sample_rate = 48000;
    let renderer = OfflineRenderer::<64>::new(sample_rate)?;
    let patch = r#"
~movement: sin 0.2 >> mul 0.4 >> add 0.5
~env: ~gate >> adsr 0.02 0.12 0.65 0.25
~cutoff: ~env >> mul 4500 >> add 300
~osc: wavetable \basic ~pitch ~movement
out: ~osc >> lpf ~cutoff 0.7 >> mul ~env >> mul ~velocity >> mul 0.15
"#;
    let notes = [
        (0.013, 1.1, 48., 0.8),
        (0.127, 1.0, 55., 0.7),
        (0.239, 0.9, 60., 0.7),
        (1.327, 1.2, 52., 0.8),
        (1.441, 1.1, 59., 0.7),
        (1.553, 1.0, 64., 0.7),
        (2.731, 0.6, 55., 0.8),
        (2.845, 0.6, 62., 0.7),
        (2.957, 0.6, 67., 0.7),
    ]
    .map(
        |(onset_seconds, duration_seconds, midi_pitch, velocity)| NoteEvent {
            onset_seconds,
            duration_seconds,
            midi_pitch,
            velocity,
        },
    );
    let audio = renderer.render_notes(patch, &notes, sample_rate * 4, 0.35)?;
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "glicol-demo.wav".into());
    let mut out = BufWriter::new(File::create(&path)?);
    let data_bytes = (audio.len() * 4) as u32;
    out.write_all(b"RIFF")?;
    out.write_all(&(36 + data_bytes).to_le_bytes())?;
    out.write_all(b"WAVEfmt ")?;
    out.write_all(&16u32.to_le_bytes())?;
    out.write_all(&1u16.to_le_bytes())?;
    out.write_all(&2u16.to_le_bytes())?;
    out.write_all(&(sample_rate as u32).to_le_bytes())?;
    out.write_all(&(sample_rate as u32 * 4).to_le_bytes())?;
    out.write_all(&4u16.to_le_bytes())?;
    out.write_all(&16u16.to_le_bytes())?;
    out.write_all(b"data")?;
    out.write_all(&data_bytes.to_le_bytes())?;
    for frame in &audio {
        for sample in frame {
            out.write_all(
                &((sample.clamp(-1., 1.) * i16::MAX as f32).round() as i16).to_le_bytes(),
            )?;
        }
    }
    out.flush()?;
    let peak = audio.iter().flatten().map(|x| x.abs()).fold(0., f32::max);
    println!(
        "{} frames, {} Hz, stereo, peak {:.5}: {path}",
        audio.len(),
        sample_rate,
        peak
    );
    Ok(())
}
