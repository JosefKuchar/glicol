use crate::{Buffer, Input, Message, Node};
use fasteval::{Compiler, Evaler};
use hashbrown::HashMap;
use std::collections::BTreeMap;

/// Stateful per-sample expression processor. Use `try_code` for fallible construction.
pub struct Eval<const N: usize> {
    pub sr: usize,
    pub bpm: f32,
    phase: usize,
    map: BTreeMap<String, f64>,
    compiled: Vec<(Option<String>, fasteval::Instruction, fasteval::Slab)>,
    input_order: Vec<usize>,
    error: Option<String>,
}
impl<const N: usize> Default for Eval<N> {
    fn default() -> Self {
        Self::new()
    }
}
impl<const N: usize> Eval<N> {
    pub fn new() -> Self {
        Self {
            sr: 44100,
            bpm: 120.,
            phase: 0,
            map: BTreeMap::from_iter([
                ("x".into(), 0.),
                ("y".into(), 0.),
                ("z".into(), 0.),
                ("sr".into(), 44100.),
                ("phase".into(), 0.),
                ("in".into(), 0.),
            ]),
            compiled: vec![],
            input_order: vec![],
            error: None,
        }
    }
    pub fn sr(mut self, sr: usize) -> Self {
        self.sr = sr.max(1);
        self.map.insert("sr".into(), self.sr as f64);
        self
    }
    pub fn bpm(mut self, bpm: f32) -> Self {
        self.bpm = bpm;
        self
    }
    pub fn try_code(mut self, code: &str) -> Result<Self, String> {
        self.apply_code(code)?;
        Ok(self)
    }
    pub fn code(mut self, code: &str) -> Self {
        if let Err(e) = self.apply_code(code) {
            self.error = Some(e);
        }
        self
    }
    fn apply_code(&mut self, code: &str) -> Result<(), String> {
        let parser = fasteval::Parser::new();
        let mut compiled = Vec::new();
        let mut namespace = self.map.clone();
        for line in code.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            let (var, expr) = match line.split_once(":=") {
                Some((var, expr)) => {
                    let var = var.trim();
                    if var.is_empty() || !var.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    {
                        return Err("invalid eval variable".into());
                    }
                    (Some(var.to_owned()), expr.trim())
                }
                None => (None, line),
            };
            let mut slab = fasteval::Slab::new();
            let ins = parser
                .parse(expr, &mut slab.ps)
                .map_err(|e| e.to_string())?
                .from(&slab.ps)
                .compile(&slab.ps, &mut slab.cs);
            // Resolve names and functions before installing a program. Stateful results are discarded.
            let value = ins.eval(&slab, &mut namespace).map_err(|e| e.to_string())?;
            if let Some(ref var) = var {
                namespace.insert(var.clone(), value);
            }
            compiled.push((var, ins, slab));
        }
        if compiled.is_empty() {
            return Err("empty eval program".into());
        }
        self.compiled = compiled;
        self.error = None;
        Ok(())
    }
}
impl<const N: usize> Node<N> for Eval<N> {
    fn process(&mut self, inputs: &mut HashMap<usize, Input<N>>, output: &mut [Buffer<N>]) {
        for out in output.iter_mut() {
            out.fill(0.);
        }
        if self.error.is_some() {
            return;
        }
        for i in 0..N {
            if let Some(input) = self
                .input_order
                .first()
                .and_then(|id| inputs.get(id))
                .or_else(|| inputs.values().next())
            {
                self.map.insert("in".into(), input.buffers()[0][i] as f64);
            }
            self.map.insert("phase".into(), self.phase as f64);
            let mut value = 0.;
            for (var, ins, slab) in &self.compiled {
                value = match ins.eval(slab, &mut self.map) {
                    Ok(v) if v.is_finite() && (v as f32).is_finite() => v,
                    Ok(_) => {
                        self.error = Some("eval produced a nonfinite sample".into());
                        return;
                    }
                    Err(e) => {
                        self.error = Some(e.to_string());
                        return;
                    }
                };
                if let Some(var) = var {
                    self.map.insert(var.clone(), value);
                }
            }
            for out in output.iter_mut() {
                out[i] = value as f32;
            }
            self.phase += 1;
        }
    }
    fn send_msg(&mut self, info: Message) {
        match info {
            Message::SetToSymbol(0, code) => {
                if let Err(e) = self.apply_code(&code) {
                    self.error = Some(e)
                }
            }
            Message::Index(i) => self.input_order.push(i),
            Message::IndexOrder(p, i) => self.input_order.insert(p.min(self.input_order.len()), i),
            Message::ResetOrder => self.input_order.clear(),
            _ => {}
        }
    }
    fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }
}
