use crate::{BoxedNodeSend, Buffer, Input, Message, Node, NodeData};
use hashbrown::HashMap;
use rhai::{Array, Dynamic, Engine, OptimizationLevel, Scope, AST};

pub struct Meta<const N: usize> {
    sr: usize,
    // phase: usize,
    pub backup: String,
    ast: AST,
    scope: Scope<'static>,
    engine: Engine,
    input_order: Vec<usize>,
    error: Option<String>,
}

impl<const N: usize> Default for Meta<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Meta<N> {
    pub fn new() -> Self {
        let phase: usize = 0;
        let mut scope = Scope::new();
        let output = Vec::<Dynamic>::with_capacity(N);

        scope
            .push("phase", phase as f32)
            .push("x0", 0.0_f32)
            .push("x1", 0.0_f32)
            .push("x2", 0.0_f32)
            .push("x3", 0.0_f32)
            .push("y0", 0.0_f32)
            .push("y1", 0.0_f32)
            .push("y2", 0.0_f32)
            .push("y3", 0.0_f32)
            .push("z0", 0.0_f32)
            .push("z1", 0.0_f32)
            .push("z2", 0.0_f32)
            .push("z3", 0.0_f32)
            .push("a", 0.0_f32)
            .push("b", 0.0_f32)
            .push("c", 0.0_f32)
            .push("d", 0.0_f32)
            .push("e", 0.0_f32)
            .push("f", 0.0_f32)
            .push("g", 0.0_f32)
            .push("h", 0.0_f32)
            .push("i", 0.0_f32)
            .push("j", 0.0_f32)
            .push("k", 0.0_f32)
            .push("l", 0.0_f32)
            .push("m", 0.0_f32)
            .push("n", 0.0_f32)
            .push("o", 0.0_f32)
            .push("p", 0.0_f32)
            .push("q", 0.0_f32)
            .push("r", 0.0_f32)
            .push("s", 0.0_f32)
            .push("t", 0.0_f32)
            .push("u", 0.0_f32)
            .push("v", 0.0_f32)
            .push("w", 0.0_f32)
            .push("x", 0.0_f32)
            .push("y", 0.0_f32)
            .push("z", 0.0_f32)
            .push("freq", 0.0_f32)
            .push("freq2", 0.0_f32)
            .push("output", output);

        let mut engine = Engine::new();
        engine.set_optimization_level(OptimizationLevel::Full);
        engine.set_max_operations(100_000);
        let ast = engine.compile("").unwrap();

        Self {
            sr: 44100,
            engine,
            scope,
            backup: "".to_owned(),
            ast,
            // phase,
            input_order: Vec::new(),
            error: None,
        }
    }

    pub fn sr(self, sr: usize) -> Self {
        Self { sr, ..self }
    }

    pub fn try_code(mut self, code: &str) -> Result<Self, String> {
        self.ast = self.engine.compile(code).map_err(|e| e.to_string())?;
        self.error = None;
        Ok(self)
    }
    pub fn code(mut self, code: &str) -> Self {
        match self.engine.compile(code) {
            Ok(ast) => {
                self.ast = ast;
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
        self
    }
}

impl<const N: usize> Node<N> for Meta<N> {
    fn process(&mut self, inputs: &mut HashMap<usize, Input<N>>, output: &mut [Buffer<N>]) {
        for out in output.iter_mut() {
            out.fill(0.0);
        }
        if self.error.is_some() {
            return;
        }
        if !inputs.is_empty() {
            let arr: Array = inputs[&self.input_order[0]].buffers()[0]
                .iter()
                .map(|f| Dynamic::from_float(*f))
                .collect();

            self.scope.set_or_push("input", arr);
            // self.engine.optimize_ast();
        }

        match self
            .engine
            .eval_ast_with_scope::<Array>(&mut self.scope, &self.ast)
        {
            Ok(result) => {
                if result.len() != N {
                    self.error = Some(format!(
                        "meta returned {} samples; expected {N}",
                        result.len()
                    ));
                    return;
                }
                for (i, res) in result.iter().enumerate() {
                    match res.as_float() {
                        Ok(v) if v.is_finite() => {
                            for out in output.iter_mut() {
                                out[i] = v;
                            }
                        }
                        _ => {
                            self.error = Some("meta output must contain finite floats".into());
                            return;
                        }
                    }
                }
            }
            Err(e) => self.error = Some(e.to_string()),
        }

        // self.phase += N;
    }
    fn send_msg(&mut self, info: Message) {
        match info {
            Message::SetToSymbol(0, s) => match self.engine.compile(s) {
                Ok(ast) => {
                    self.ast = ast;
                    self.error = None;
                }
                Err(e) => self.error = Some(e.to_string()),
            },
            Message::Index(i) => self.input_order.push(i),
            Message::IndexOrder(pos, index) => self.input_order.insert(pos, index),
            Message::ResetOrder => {
                self.input_order.clear();
            }
            _ => {}
        }
    }

    fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }

    fn to_boxed_nodedata(mut self, channels: usize) -> NodeData<BoxedNodeSend<N>, N> {
        self.scope.push("sr", self.sr as f32);
        NodeData::multi_chan_node(channels, BoxedNodeSend::<N>::new(self))
    }
}
