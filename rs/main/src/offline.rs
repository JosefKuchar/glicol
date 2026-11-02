//! Literal, non-looping note performances rendered with independent patch voices.
use crate::{Engine, EngineError};
use glicol_synth::{
    oscillator::WavetableBank,
    signal::{ConstSig, Gate},
    BoxedNodeSend,
};
use hashbrown::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy)]
pub struct NoteEvent {
    pub onset_seconds: f64,
    pub duration_seconds: f64,
    pub midi_pitch: f32,
    pub velocity: f32,
}

/// An offline host for patches using `~pitch` (Hz), `~gate`, and `~velocity` (0..1).
/// Each note gets fresh DSP state; overlaps, including repeated pitches, are independent.
/// Effects belong to each voice. Shared track/master effects are a separate graph.
pub struct OfflineRenderer<const N: usize> {
    sample_rate: usize,
    wave_tables: HashMap<String, Arc<WavetableBank>>,
}
impl<const N: usize> OfflineRenderer<N> {
    pub fn new(sample_rate: usize) -> Result<Self, EngineError> {
        if N == 0 || !(8000..=192000).contains(&sample_rate) {
            return Err(EngineError::InvalidParameter(
                "offline rendering needs N > 0 and sample rate 8000..192000".into(),
            ));
        }
        Ok(Self {
            sample_rate,
            wave_tables: HashMap::new(),
        })
    }
    pub fn add_wavetable(
        &mut self,
        name: &str,
        bank: Arc<WavetableBank>,
    ) -> Result<(), EngineError> {
        let mut engine = Engine::<N>::new();
        engine.add_wavetable(name, bank.clone())?;
        self.wave_tables.insert(name.into(), bank);
        Ok(())
    }
    /// Render exactly `frames` interleaved stereo frames. Times quantize to nearest sample.
    /// `release_seconds` is the explicit maximum tail budget for each voice.
    pub fn render_notes(
        &self,
        patch: &str,
        notes: &[NoteEvent],
        frames: usize,
        release_seconds: f64,
    ) -> Result<Vec<[f32; 2]>, EngineError> {
        let invalid = |s: &str| EngineError::InvalidParameter(s.into());
        if frames > self.sample_rate * 600
            || !release_seconds.is_finite()
            || !(0.0..=600.0).contains(&release_seconds)
        {
            return Err(invalid("render and release must fit within 600 seconds"));
        }
        let ast = glicol_parser::get_ast(patch)?;
        if ["~pitch", "~gate", "~velocity"]
            .iter()
            .any(|name| ast.nodes.contains_key(name))
        {
            return Err(invalid(
                "patch must reference, not define, the reserved ~pitch/~gate/~velocity controls",
            ));
        }
        for note in notes {
            if !note.onset_seconds.is_finite()
                || note.onset_seconds < 0.
                || note.onset_seconds > 600.
                || !note.duration_seconds.is_finite()
                || !(0.0..=600.).contains(&note.duration_seconds)
                || !note.midi_pitch.is_finite()
                || !(0.0..=127.).contains(&note.midi_pitch)
                || !note.velocity.is_finite()
                || !(0.0..=1.).contains(&note.velocity)
            {
                return Err(invalid(
                    "invalid note onset, duration, MIDI pitch or velocity",
                ));
            }
        }
        let code = format!("{patch}\n~pitch: sig 0\n~gate: sig 0\n~velocity: sig 0\n");
        // Validate even an empty performance; never accept a broken patch as a silent item.
        let make_engine = || -> Result<Engine<N>, EngineError> {
            let mut engine = Engine::<N>::new();
            engine.try_set_sr(self.sample_rate)?;
            for (name, bank) in &self.wave_tables {
                engine.add_wavetable(name, bank.clone())?;
            }
            engine.update_with_code(&code)?;
            Ok(engine)
        };
        let mut first_engine = Some(make_engine()?);
        let mut result = vec![[0.; 2]; frames];
        let tail = (release_seconds * self.sample_rate as f64).round() as usize;
        for note in notes {
            let start = (note.onset_seconds * self.sample_rate as f64).round() as usize;
            let duration = (note.duration_seconds * self.sample_rate as f64).round() as usize;
            if start >= frames || duration == 0 || note.velocity == 0. {
                continue;
            }
            let mut engine = match first_engine.take() {
                Some(e) => e,
                None => make_engine()?,
            };
            let controls = [
                (
                    "~pitch",
                    BoxedNodeSend::new(ConstSig::new(
                        440. * 2f32.powf((note.midi_pitch - 69.) / 12.),
                    )),
                ),
                ("~gate", BoxedNodeSend::new(Gate::new(0, duration))),
                (
                    "~velocity",
                    BoxedNodeSend::new(ConstSig::new(note.velocity)),
                ),
            ];
            for (name, node) in controls {
                let id = engine.index_info[name][0];
                engine.context.graph[id].node = node;
            }
            let len = duration.saturating_add(tail).min(frames - start);
            let mut offset = 0;
            while offset < len {
                let block = engine.try_next_block(vec![])?;
                let count = N.min(len - offset);
                for i in 0..count {
                    for c in 0..2 {
                        result[start + offset + i][c] += block[c][i];
                    }
                }
                offset += count;
            }
        }
        if result.iter().flatten().any(|x| !x.is_finite()) {
            return Err(EngineError::ProcessingError("nonfinite mix".into()));
        }
        Ok(result)
    }
}
