# Synthesis work in this fork

This fork adds synthesis building blocks and a checked offline host for paired
patch/performance/audio generation. It is an extension of Glicol, not a complete
Serum implementation. The existing music-decompiler renderer is a separate
integration and has not been migrated by these changes.

## Patch language

```text
~width: sin 0.3 >> mul 0.2 >> add 0.5
out: pulse 110 ~width >> mul 0.2
```

`pulse frequency width` uses PolyBLEP corrections and accepts numeric values or
signal references for both controls. Frequency is Hz and width is a fraction of
a cycle. Controls run per sample. Width is constrained by the phase step to
keep the two discontinuities separated. The existing `saw` and `squ` nodes now
also use PolyBLEP, retaining their syntax and legacy zero-input sample-and-hold
frequency behavior. Use `pulse` or `wavetable` when zero frequency must freeze
phase exactly. Phase increments are bounded below Nyquist; negative frequencies
wrap correctly. Triangle remains the original waveform.

```text
~morph: sin 0.2 >> mul 0.4 >> add 0.5
out: wavetable \basic 220 ~morph >> mul 0.2
```

`wavetable bank frequency position` is a periodic oscillator, not a sample player.
`\basic` contains sine, saw and square frames; position 0..1 interpolates between
frames. Frequency and position accept signals and run per sample. Immutable
banks contain harmonic-limited mip levels, selected and crossfaded according to
frequency, with linear interpolation within each waveform. Bank construction
runs before rendering; oscillator processing does not allocate tables.

Register custom banks through Rust before loading a patch:

```rust
use std::sync::Arc;
use glicol::Engine;
use glicol_synth::oscillator::WavetableBank;

fn main() -> Result<(), Box<dyn std::error::Error>> {
let bank = Arc::new(WavetableBank::from_harmonics(
    &[vec![1.0], vec![1.0, 0.4, 0.2]], 256,
)?);
let mut engine = Engine::<64>::new();
engine.try_set_sr(48000)?;
engine.add_wavetable("custom", bank)?;
engine.update_with_code("out: wavetable \\custom 220 0.5")?;
let block = engine.try_next_block(vec![])?;
Ok(())
}
```

`from_harmonics` uses sine harmonic amplitudes (fundamental first).
`from_frames` accepts arbitrary finite single-cycle samples, including phase
and DC, computes their Fourier coefficients and builds mip levels. The exact
Nyquist component is discarded. Frames must have equal power-of-two lengths
in 32..2048; 1..256 frames are accepted. This direct Fourier preparation is
intended for cached offline construction, not live table rebuilding.

`gate onset duration` produces one non-looping gate, with times in seconds from
node creation, rounded to the nearest sample. `Gate::new(onset, duration)` uses
integer sample counts in Rust. `gate`, `pulse` and `wavetable` are source nodes
and start a chain; route them into effects or modulation references.

## Literal note performances

`OfflineRenderer<N>` accepts independent note events with onset seconds,
duration seconds, MIDI pitch and velocity. Patches reference three reserved
controls and must not define them:

```text
~env: ~gate >> adsr 0.01 0.1 0.6 0.2
out: wavetable \basic ~pitch 0.5 >> mul ~env >> mul ~velocity >> mul 0.2
```

The renderer creates fresh per-note DSP state and mixes at known sample offsets.
Overlapping notes, including repeated pitches, have independent gates and release
tails. `~pitch` is Hz, `~gate` is 0/1, and `~velocity` is 0..1. The patch decides
how velocity changes the sound; it is not automatically applied a second time.
Note duration controls the gate; the caller supplies an explicit release-tail
budget. Results are exactly the requested number of stereo float32 frames,
without sleeping, an audio device or looping arrangement semantics.

Sample rates are 8–192 kHz. Clips and individual note/tail times are bounded to
600 seconds; MIDI pitch is 0..127 (fractional pitches are accepted). Output is
not normalized or limited. Custom banks can also be registered on the offline
renderer. Effects run per voice. Shared track/master effects, real-time voice
allocation, sustain pedal, MPE and performance automation remain host work.

For a runnable polyphonic WAV example:

```sh
cd rs
cargo run --release -p glicol --example synthesis_offline -- /tmp/glicol-demo.wav
```

## Correctness and error handling

- `Points` handles the first/last sample, holds a non-looping endpoint, interpolates
  the wrap segment for looping curves, and resolves coincident sample times with
  the last value. Empty/invalid language curves are rejected. The default bar
  span is one. Rebuild automation after changing BPM; stored point positions are
  computed at construction.
- ADSR retriggering starts from the current output level. Zero-length stages have
  defined behavior. Per-voice independent gates avoid shared-envelope note-offs.
- LP/HP cutoff references update coefficients per sample. Filters use the current
  input sample, fixing a stale state variable. Q remains numeric, bounded to
  0.01..100; dynamic cutoff is clamped to 0..0.499 times sample rate. These remain
  two-pole filters, not a new ladder/SVF family.
- Compound drum/synth constructors and plate reverb receive the requested sample
  rate. Plate sample-count taps scale from their 44.1 kHz tuning and damping
  coefficients are adjusted. Millisecond/all-pass fractional delay reads use
  bounded, correctly weighted circular interpolation. Static zero delay bypasses.
- Set sample rate before loading a patch. Reset before changing it; existing
  graphs are not silently left at a mixture of old and new rates. Registered
  wavetable banks survive reset; loaded samples follow upstream reset behavior.
- `eval` preserves the full expression, compiles fallibly and reports evaluation
  failures/nonfinite values. Rhai `meta` reports compile/runtime/output errors,
  requires exactly N finite floats per block, and has a 100,000 operation budget
  per block. It cannot silently retain stale output after an error.
- Unsupported `expr`/`reverb` constructs return `UnsupportedNode`. Supported
  plate reverb remains `plate`. Malformed message targets/indices and malformed
  `MsgSynth` timestamps report errors. `MsgSynth`'s reversed envelope branches
  are corrected, but it remains a fixed legacy one-shot saw instrument.
- Missing references are validated before mutating the graph. Invalid source/effect
  ordering is rejected. Existing valid patch syntax is retained; incorrect
  programs previously accepted as silence may now be rejected.

Use `update_with_code`, `try_send_msg` and **`try_next_block`** for checked hosts.
A failed patch update, failed legacy message, script error or nonfinite block
prevents checked rendering until reset or a successful patch update. Check
returned errors; do not keep the previous recording as a training item.
`next_block` remains the compatibility API and does not provide those checks.
This is not a sandbox for arbitrary custom Rust nodes: their panics and resource
usage are the implementer's responsibility. Not every legacy pattern-node
parameter has been comprehensively validated.

## Validation and remaining DSP work

See [VALIDATION_SYNTHESIS.md](VALIDATION_SYNTHESIS.md) for the executed checks,
results and their limits.

Run native checks from the repository root:

```sh
cargo test --manifest-path rs/Cargo.toml -p glicol -p glicol_parser -p glicol_synth \
  --lib --tests --features glicol/use-meta,glicol/use-samples
cargo test --release --manifest-path rs/Cargo.toml -p glicol \
  --test dsp_regressions --features use-meta,use-samples
```

The regression suite checks sample timing, same-pitch overlaps, velocity, releases,
ADSR retriggering, curve edges, rate-correct drums, fractional/stereo delay,
block-invariant filter/PWM/wavetable modulation, custom-bank morph/phase/DC,
explicit script errors, and reduced folded-harmonic energy for a high saw note.
It also renders representative patches and plate at 16 and 48 kHz. These are
bounded numerical tests; they do not establish subjective quality or throughput.

PolyBLEP and table mipmaps reduce aliasing; fast FM/PWM/morphing and nonlinear
effects can still create aliases. Oversampling and high-quality downsampling
remain separate work before a broad 16 kHz training distribution is adopted.
Plate still has graph/block-based feedback: its behavior can depend on block
size. It is not sample-wise recursive DSP. Implement feedback oscillators,
resonators and physical models inside dedicated stateful nodes.

Granular/spectral synthesis, convolution, a complete modulation matrix,
compressors/limiters, efficient unison, additional filter families, and polished
preset instruments are still extensions to build. This fork now has useful
foundations and regression coverage, not all of those finished features.
