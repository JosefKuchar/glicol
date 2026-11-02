# Synthesis validation — 2026-11-02

Local fork based on upstream `0317db2e3f157b911bfc28c72ad5db90db963f24`.
Validation used Linux x86_64 and Rust 1.92.0. Implementation and scope are in
[SYNTHESIS.md](SYNTHESIS.md); the new checks are in
[rs/main/tests/dsp_regressions.rs](rs/main/tests/dsp_regressions.rs).

- Native library/integration suite with `use-meta,use-samples`: **53 passed**,
  including 30 synthesis regressions and four raw resonant-filter regressions.
- Default-feature native library/integration suite: **51 passed** (Rhai-specific
  checks excluded).
- All 30 DSP regressions also passed under `--release`.
- `glicol-wasm` compiled for `wasm32-unknown-unknown` with its normal features.
  This is compilation validation, not an executed browser/audio-worklet test.
- Native `glicol` compiled with `use-meta,use-samples,bela`; no Bela hardware
  execution was performed.
- The release `synthesis_offline` example produced a valid 192,000-frame,
  48 kHz stereo, 16-bit PCM WAV, with pre-conversion peak 0.16531.
- Two concurrent independent example processes produced byte-identical WAVs:
  SHA-256 `c29cff331a5432dbaa834397a5b861d1ab2ae1ab03b20fafa4551d08912461e2`.
  This checks this patch/performance on this machine, not cross-platform or
  universal node determinism.
- Formatting of changed/new Rust files and `git diff --check` passed.

Existing parser integration tests had stale `NumberOrRef` expectations for the
integer sequencer AST and did not compile. Their expectations now use the
existing `UsizeOrRef` type; all 16 parser integration tests pass.

Commands, from the fork root:

```sh
cargo test --manifest-path rs/Cargo.toml -p glicol -p glicol_parser -p glicol_synth \
  --lib --tests --features glicol/use-meta,glicol/use-samples
cargo test --manifest-path rs/Cargo.toml -p glicol -p glicol_parser -p glicol_synth \
  --lib --tests
cargo test --release --manifest-path rs/Cargo.toml -p glicol \
  --test dsp_regressions --features use-meta,use-samples
cargo check --manifest-path rs/Cargo.toml -p glicol-wasm --target wasm32-unknown-unknown
cargo check --manifest-path rs/Cargo.toml -p glicol --features use-meta,use-samples,bela
cargo run --release --manifest-path rs/Cargo.toml -p glicol \
  --example synthesis_offline -- /tmp/glicol-demo.wav
```

These changes affect rendered sound and need a new engine/distribution version
before training data is regenerated. Music-decompiler's Rustel renderer, Python
binding, training setup and checkpoints have not been changed. Broad listening,
throughput, Python worker and resampler qualification remain integration work.

## PR #196 regression coverage

[Upstream PR #196](https://github.com/chaosprint/glicol/pull/196) reports a missing
current-input term in both resonant filters. The synthesis commit already
removed the unused `x0` state and uses the current sample in the difference
equation. No cherry-pick was needed on top of the per-sample modulation rewrite.

The four [raw-node filter tests](rs/synth/tests/resonant_filters.rs) exercise
fixed and signal-driven cutoff, including the reported 65 Hz saw / 90 Hz
high-pass case, at 16, 44.1 and 48 kHz. They also check the first output sample
of an impulse so future timing/gain regressions cannot hide behind a steady-state
check. All four fail against the original upstream commit and pass in this fork.

```sh
cargo test --manifest-path rs/Cargo.toml -p glicol_synth --test resonant_filters
```
