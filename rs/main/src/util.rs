use glicol_synth::{
    compound::{Bd, Hh, SawSynth, Sn, SquSynth, TriSynth},
    delay::{DelayMs, DelayN},
    effect::{Balance, Pan, Plate},
    envelope::{Adsr, EnvPerc},
    filter::{AllPassFilterGain, OnePole, ResonantHighPassFilter, ResonantLowPassFilter},
    operator::{Add, Mul},
    oscillator::{SawOsc, SinOsc, SquOsc, TriOsc},
    sequencer::{Arrange, Choose, Sequencer, Speed},
    signal::{ConstSig, Impulse, Noise, Points},
    synth::{MsgSynth, PatternSynth},
    Node, Pass, Sum2,
};
use glicol_synth::{
    oscillator::{PulseOsc, WavetableBank, WavetableOsc},
    signal::Gate,
};
use std::sync::Arc;

use glicol_parser::{
    nodes::{self, Component, UsizeOrRef},
    ToInnerOwned as _,
};

#[cfg(feature = "use-meta")]
use glicol_synth::dynamic::Meta;

use glicol_synth::dynamic::Eval;

#[cfg(feature = "use-samples")]
use glicol_synth::sampling::{PSampler, Sampler};

use crate::EngineError;
use glicol_synth::{BoxedNodeSend, NodeData}; //, Processor, Buffer, Input, Node
use hashbrown::HashMap;

pub type GlicolNodeData<const N: usize> = NodeData<BoxedNodeSend<N>, N>;

#[allow(unused_variables, unused_mut)]
pub fn makenode<const N: usize>(
    component: &Component<'_>,
    samples_dict: &HashMap<String, (&'static [f32], usize, usize)>,
    wave_tables: &HashMap<String, Arc<WavetableBank>>,
    sr: usize,
    bpm: f32,
    seed: usize,
) -> Result<(GlicolNodeData<N>, Vec<String>), EngineError> {
    validate_component(component, sr, bpm)?;
    let (nodedata, reflist) = match component {
        Component::Gate(nodes::Gate { onset, duration }) => (
            Gate::new(
                (*onset as f64 * sr as f64).round() as usize,
                (*duration as f64 * sr as f64).round() as usize,
            )
            .to_boxed_nodedata(1),
            vec![],
        ),
        Component::Pulse(nodes::Pulse { frequency, width }) => {
            let mut refs = vec![];
            let (frequency, freq_ref) = control_param(frequency, &mut refs);
            let (width, width_ref) = control_param(width, &mut refs);
            (
                PulseOsc::new(frequency, width, sr)
                    .controls(freq_ref, width_ref)
                    .to_boxed_nodedata(1),
                refs,
            )
        }
        Component::Wavetable(nodes::Wavetable {
            bank,
            frequency,
            position,
        }) => {
            let name = bank.trim_matches(['\\', '\'']);
            let bank = wave_tables.get(name).ok_or_else(|| {
                EngineError::InvalidParameter(format!("unknown wavetable bank {name}"))
            })?;
            let mut refs = vec![];
            let (frequency, freq_ref) = control_param(frequency, &mut refs);
            let (position, position_ref) = control_param(position, &mut refs);
            (
                WavetableOsc::new(bank.clone(), frequency, position, sr)
                    .controls(freq_ref, position_ref)
                    .to_boxed_nodedata(1),
                refs,
            )
        }
        #[cfg(feature = "use-samples")]
        Component::PSampler(psampler) => {
            let mut samples_dict_selected = HashMap::new();
            let (pattern, span) = match psampler {
                nodes::PSampler::Event(_) => {
                    return Err(EngineError::UnsupportedNode("psampler event".into()))
                }
                nodes::PSampler::Pattern(ref pat) => (&pat.event, pat.span),
            };

            let pattern = pattern
                .val_times
                .iter()
                .map(|(val, time)| {
                    let value = match &val {
                        nodes::EventValue::Number(_) => String::new(),
                        nodes::EventValue::Symbol(sym) => sym.to_string(),
                    };

                    if !samples_dict.contains_key(&value) {
                        return Err(EngineError::NonExistSample(value.clone()));
                    } else {
                        samples_dict_selected.insert(value.clone(), samples_dict[&value]);
                    }

                    Ok((value, *time))
                })
                .collect::<Result<Vec<_>, EngineError>>()?;

            (
                PSampler::new(samples_dict_selected, sr, bpm, vec![], pattern, span)
                    .to_boxed_nodedata(2),
                vec![],
            )
        }
        Component::Points(nodes::Points {
            points,
            span,
            is_looping,
        }) => (
            Points::new()
                .bpm(bpm)
                .sr(sr)
                .span(*span)
                .points(points.to_vec())
                .is_looping(*is_looping)
                .to_boxed_nodedata(1),
            vec![],
        ),
        Component::MsgSynth(nodes::MsgSynth {
            symbol,
            attack,
            decay,
        }) => (
            MsgSynth::new()
                .sr(sr)
                .attack(*attack)
                .decay(*decay)
                .to_boxed_nodedata(1),
            vec![],
        ),
        Component::PatternSynth(nodes::PatternSynth { symbol, span }) => {
            let pattern = symbol.replace('`', "");
            let events = pattern
                .split(',')
                .map(|event| {
                    let values = event
                        .split_whitespace()
                        .map(str::parse::<f32>)
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|e| EngineError::InvalidParameter(e.to_string()))?;
                    if values.len() != 2 || values.iter().any(|v| !v.is_finite() || *v < 0.0) {
                        return Err(EngineError::InvalidParameter(
                            "psynth expects time pitch pairs".into(),
                        ));
                    }
                    Ok((values[0], values[1]))
                })
                .collect::<Result<Vec<_>, EngineError>>()?;

            (
                PatternSynth::new(events).sr(sr).to_boxed_nodedata(1),
                vec![],
            )
        }

        #[cfg(feature = "bela")]
        Component::Adc(nodes::Adc { port }) => (
            NodeData::new1(BoxedNodeSend::new(Pass {})),
            vec![format!("~adc{}", port + 1)],
        ),

        #[cfg(feature = "use-samples")]
        Component::Sp(nodes::Sp { sample_sym }) => {
            let Some(sample) = samples_dict.get(*sample_sym) else {
                return Err(EngineError::NonExistSample(sample_sym.to_string()));
            };

            (Sampler::new(*sample, sr).to_boxed_nodedata(2), vec![])
        }

        #[cfg(feature = "use-meta")]
        Component::Meta(nodes::Meta { code }) => (
            Meta::new()
                .sr(sr)
                .try_code(code.code)
                .map_err(EngineError::InvalidParameter)?
                .to_boxed_nodedata(1),
            vec![],
        ),
        // "expr" => {
        //     match &paras[0] {
        //         GlicolPara::Symbol(s) => {
        //             (Expr::new().sr(sr).code(s.to_owned()).to_boxed_nodedata(1), vec![])
        //         },
        //         _ => unimplemented!()
        //     }
        // },
        Component::Eval(nodes::Eval { code }) => (
            Eval::new()
                .sr(sr)
                .try_code(code.code)
                .map_err(EngineError::InvalidParameter)?
                .to_boxed_nodedata(1),
            vec![],
        ),
        Component::Lpf(nodes::Lpf { signal, qvalue }) => {
            let mut reflist = vec![];
            let data = match signal {
                nodes::Signal::Number(v) => ResonantLowPassFilter::new()
                    .cutoff(*v)
                    .q(*qvalue)
                    .sr(sr)
                    .to_boxed_nodedata(1),
                nodes::Signal::Reference(s) => {
                    reflist.push(s.to_string());
                    ResonantLowPassFilter::new()
                        .q(*qvalue)
                        .sr(sr)
                        .to_boxed_nodedata(1)
                }
                nodes::Signal::Pattern(nodes::Pattern { event, span }) => {
                    let pattern = event
                        .val_times
                        .iter()
                        .map(|(val, time)| {
                            let value = match val {
                                nodes::EventValue::Number(num) => *num,
                                nodes::EventValue::Symbol(_) => 100.0,
                            };
                            (value, *time)
                        })
                        .collect();

                    // println!("pattern {:?}", pattern);
                    ResonantLowPassFilter::new()
                        .q(*qvalue)
                        .pattern(pattern)
                        .span(*span)
                        .bpm(bpm)
                        .sr(sr)
                        .to_boxed_nodedata(1)
                }
                nodes::Signal::Event(_) => {
                    panic!("An event as a parameter to lpf is not currently supported")
                }
            };
            (data, reflist)
        }
        Component::Balance(nodes::Balance { left, right }) => {
            let data = Balance::new().to_boxed_nodedata(2);
            let reflist = vec![left.to_string(), right.to_string()];
            (data, reflist)
        }
        Component::Rhpf(nodes::Rhpf { cutoff, qvalue }) => {
            let data = ResonantHighPassFilter::new()
                .cutoff(match cutoff {
                    nodes::NumberOrRef::Number(v) => *v,
                    nodes::NumberOrRef::Ref(_) => 100.0,
                })
                .q(*qvalue)
                .to_boxed_nodedata(1);

            let mut reflist = vec![];
            if let nodes::NumberOrRef::Ref(s) = cutoff {
                reflist.push(s.to_string());
            };
            (data, reflist)
        }
        Component::ApfmsGain(nodes::ApfmsGain { delay, gain }) => {
            let data = AllPassFilterGain::new()
                .sr(sr)
                .delay(match delay {
                    nodes::NumberOrRef::Number(v) => *v,
                    nodes::NumberOrRef::Ref(_) => 0.0,
                })
                .gain(*gain)
                .to_boxed_nodedata(2);

            let mut reflist = vec![];
            if let nodes::NumberOrRef::Ref(s) = delay {
                reflist.push(s.to_string());
            };
            (data, reflist)
        }
        // "reverb" => {
        //     let data = Reverb::new().sr(sr).to_boxed_nodedata(2);
        //     let reflist = vec![];
        //     (data, reflist)
        // },
        Component::EnvPerc(nodes::EnvPerc { attack, decay }) => (
            EnvPerc::new()
                .sr(sr)
                .attack(*attack)
                .decay(*decay)
                .to_boxed_nodedata(2),
            vec![],
        ),
        Component::Adsr(nodes::Adsr {
            attack,
            decay,
            sustain,
            release,
        }) => (
            Adsr::new()
                .sr(sr)
                .attack(*attack)
                .decay(*decay)
                .sustain(*sustain)
                .release(*release)
                .to_boxed_nodedata(2),
            vec![],
        ),
        Component::Tri(nodes::Tri { param }) => match param {
            nodes::NumberOrRef::Number(v) => {
                (TriOsc::new().sr(sr).freq(*v).to_boxed_nodedata(1), vec![])
            }
            nodes::NumberOrRef::Ref(s) => (
                TriOsc::new().sr(sr).freq(0.0).to_boxed_nodedata(1),
                vec![s.to_string()],
            ),
        },
        Component::Squ(nodes::Squ { param }) => match param {
            nodes::NumberOrRef::Number(v) => {
                (SquOsc::new().sr(sr).freq(*v).to_boxed_nodedata(1), vec![])
            }
            nodes::NumberOrRef::Ref(s) => (
                SquOsc::new().sr(sr).freq(0.0).to_boxed_nodedata(1),
                vec![s.to_string()],
            ),
        },
        Component::Saw(nodes::Saw { param }) => match param {
            nodes::NumberOrRef::Number(v) => {
                (SawOsc::new().sr(sr).freq(*v).to_boxed_nodedata(1), vec![])
            }
            nodes::NumberOrRef::Ref(s) => (
                SawOsc::new().sr(sr).freq(0.0).to_boxed_nodedata(1),
                vec![s.to_string()],
            ),
        },
        Component::Sin(nodes::Sin { param }) => match param {
            nodes::NumberOrRef::Number(v) => {
                (SinOsc::new().sr(sr).freq(*v).to_boxed_nodedata(1), vec![])
            }
            nodes::NumberOrRef::Ref(s) => (
                SinOsc::new().sr(sr).freq(0.0).to_boxed_nodedata(1),
                vec![s.to_string()],
            ),
        },
        Component::Plate(nodes::Plate { mix }) => (
            Plate::with_sample_rate(*mix, sr).to_boxed_nodedata(2),
            vec![],
        ),
        Component::Imp(nodes::Imp { param }) => match param {
            nodes::NumberOrRef::Number(v) => {
                (Impulse::new().sr(sr).freq(*v).to_boxed_nodedata(1), vec![])
            }
            nodes::NumberOrRef::Ref(s) => (
                Impulse::new().sr(sr).freq(0.0).to_boxed_nodedata(1),
                vec![s.to_string()],
            ),
        },
        Component::Mul(nodes::Mul { param }) => match param {
            nodes::NumberOrRef::Number(v) => (Mul::new(*v).to_boxed_nodedata(2), vec![]),
            nodes::NumberOrRef::Ref(s) => (Mul::new(0.0).to_boxed_nodedata(2), vec![s.to_string()]),
        },
        Component::Pan(nodes::Pan { param }) => match param {
            nodes::NumberOrRef::Number(v) => (Pan::new(*v).to_boxed_nodedata(2), vec![]),
            nodes::NumberOrRef::Ref(s) => (Pan::new(0.0).to_boxed_nodedata(2), vec![s.to_string()]),
        },
        Component::Delayn(nodes::Delayn { param }) => match param {
            nodes::UsizeOrRef::Usize(v) => (DelayN::new(*v).to_boxed_nodedata(2), vec![]),
            nodes::UsizeOrRef::Ref(s) => (DelayN::new(0).to_boxed_nodedata(2), vec![s.to_string()]),
        },
        Component::Delayms(nodes::Delayms { param }) => match param {
            nodes::NumberOrRef::Number(v) => (
                DelayMs::new().sr(sr).delay(*v, 2).to_boxed_nodedata(2),
                vec![],
            ),
            nodes::NumberOrRef::Ref(s) => (
                DelayMs::new().sr(sr).delay(2000., 2).to_boxed_nodedata(2),
                vec![s.to_string()],
            ),
        },
        Component::Noise(nodes::Noise { seed }) => (Noise::new(*seed).to_boxed_nodedata(1), vec![]),
        Component::Speed(nodes::Speed { speed }) => {
            (Speed::from(*speed).to_boxed_nodedata(1), vec![])
        }
        Component::Onepole(nodes::Onepole { param }) => {
            get_one_para_from_number_or_ref::<N, OnePole>(param, 1)
        }
        Component::Add(nodes::Add { param }) => get_one_para_from_number_or_ref::<N, Add>(param, 2),
        Component::ConstSig(nodes::ConstSig { value }) => {
            (ConstSig::new(*value).sr(sr).to_boxed_nodedata(1), vec![])
        }
        // todo: give sr to them
        Component::Bd(nodes::Bd { param }) => {
            let (value, refs) = match param {
                nodes::NumberOrRef::Number(v) => (*v, vec![]),
                nodes::NumberOrRef::Ref(r) => (0.1, vec![r.to_string()]),
            };
            (Bd::with_sample_rate(value, sr).to_boxed_nodedata(2), refs)
        }
        Component::Hh(nodes::Hh { param }) => {
            let (value, refs) = match param {
                nodes::NumberOrRef::Number(v) => (*v, vec![]),
                nodes::NumberOrRef::Ref(r) => (0.1, vec![r.to_string()]),
            };
            (Hh::with_sample_rate(value, sr).to_boxed_nodedata(2), refs)
        }
        Component::Sn(nodes::Sn { param }) => {
            let (value, refs) = match param {
                nodes::NumberOrRef::Number(v) => (*v, vec![]),
                nodes::NumberOrRef::Ref(r) => (0.1, vec![r.to_string()]),
            };
            (Sn::with_sample_rate(value, sr).to_boxed_nodedata(2), refs)
        }
        Component::SawSynth(nodes::SawSynth { attack, decay }) => (
            SawSynth::with_sample_rate(*attack, *decay, sr).to_boxed_nodedata(2),
            vec![],
        ),
        Component::SquSynth(nodes::SquSynth { attack, decay }) => (
            SquSynth::with_sample_rate(*attack, *decay, sr).to_boxed_nodedata(2),
            vec![],
        ),
        Component::TriSynth(nodes::TriSynth { attack, decay }) => (
            TriSynth::with_sample_rate(*attack, *decay, sr).to_boxed_nodedata(2),
            vec![],
        ),
        Component::Get(nodes::Get { reference }) => (
            NodeData::new2(BoxedNodeSend::new(Pass {})),
            vec![reference.to_string()],
        ),
        Component::Seq(nodes::Seq { events }) => {
            let mut reflist = Vec::<String>::new();
            let mut order = HashMap::new();
            let mut count = 0;
            for event in events {
                if let UsizeOrRef::Ref(s) = &event.1 {
                    // reflist: ["~a", "~b", "~a"]
                    if !reflist.iter().any(|r| r == s) {
                        reflist.push(s.to_string());
                        order.insert(s.to_string(), count);
                        count += 1;
                    }
                }
            }
            (
                Sequencer::new(events.to_inner_owned())
                    .sr(sr)
                    .bpm(bpm)
                    .ref_order(order)
                    .to_boxed_nodedata(2),
                reflist,
            )
        }
        Component::Choose(nodes::Choose { choices }) => (
            Choose::new(choices.clone(), seed as u64).to_boxed_nodedata(2),
            vec![],
        ),
        Component::Mix(nodes::Mix { nodes }) => (
            NodeData::new2(BoxedNodeSend::new(Sum2 {})),
            nodes.iter().map(ToString::to_string).collect(),
        ),
        Component::Arrange(nodes::Arrange { events }) => {
            let reflist = events
                .iter()
                .flat_map(|ev| match ev {
                    nodes::NumberOrRef::Number(_) => None,
                    nodes::NumberOrRef::Ref(s) => Some(s.to_string()),
                })
                .collect();

            (
                Arrange::new(events.to_inner_owned())
                    .sr(sr)
                    .bpm(bpm)
                    .to_boxed_nodedata(2),
                reflist,
            )
        }
        Component::Reverb(_) | Component::Expr(_) => {
            return Err(EngineError::UnsupportedNode(format!("{component:?}")))
        }
        #[cfg(not(feature = "use-samples"))]
        Component::Sp(_) | Component::PSampler(_) => {
            return Err(EngineError::UnsupportedNode(
                "sampler requires use-samples".into(),
            ))
        }
        #[cfg(not(feature = "bela"))]
        Component::Adc(_) => return Err(EngineError::UnsupportedNode("adc requires bela".into())),
        #[cfg(not(feature = "use-meta"))]
        Component::Meta(_) => {
            return Err(EngineError::UnsupportedNode(
                "meta requires use-meta".into(),
            ))
        }
    };
    Ok((nodedata, reflist))
}

fn get_one_para_from_number_or_ref<const N: usize, T>(
    param: &nodes::NumberOrRef<&str>,
    channels: usize,
) -> (NodeData<BoxedNodeSend<N>, N>, Vec<String>)
where
    T: From<f32> + Node<N> + Send + 'static,
{
    match param {
        nodes::NumberOrRef::Number(v) => (T::from(*v).to_boxed_nodedata(channels), vec![]),
        nodes::NumberOrRef::Ref(s) => (
            T::from(0.0).to_boxed_nodedata(channels),
            vec![(*s).to_owned()],
        ),
    }
}

fn control_param(param: &nodes::NumberOrRef<&str>, refs: &mut Vec<String>) -> (f32, Option<usize>) {
    match param {
        nodes::NumberOrRef::Number(v) => (*v, None),
        nodes::NumberOrRef::Ref(r) => {
            let slot = refs.len();
            refs.push(r.to_string());
            (0., Some(slot))
        }
    }
}

fn validate_component(c: &Component<'_>, sr: usize, bpm: f32) -> Result<(), EngineError> {
    let check = |v: f32, min: f32, max: f32, name: &str| {
        if v.is_finite() && v >= min && v <= max {
            Ok(())
        } else {
            Err(EngineError::InvalidParameter(format!(
                "{name} must be finite and in {min}..={max}"
            )))
        }
    };
    let signal = |p: &nodes::NumberOrRef<&str>, min, max, name| match p {
        nodes::NumberOrRef::Number(v) => check(*v, min, max, name),
        _ => Ok(()),
    };
    match c {
        Component::Gate(nodes::Gate { onset, duration }) => {
            check(*onset, 0., 86400., "gate onset")?;
            check(*duration, 0., 86400., "gate duration")?;
        }
        Component::Adsr(nodes::Adsr {
            attack,
            decay,
            sustain,
            release,
        }) => {
            for v in [attack, decay, release] {
                check(*v, 0., 600., "envelope time")?;
            }
            check(*sustain, 0., 1., "sustain")?;
        }
        Component::EnvPerc(nodes::EnvPerc { attack, decay }) => {
            check(*attack, 0., 600., "attack")?;
            check(*decay, 0., 600., "decay")?;
        }
        Component::SawSynth(nodes::SawSynth { attack, decay })
        | Component::SquSynth(nodes::SquSynth { attack, decay })
        | Component::TriSynth(nodes::TriSynth { attack, decay }) => {
            check(*attack, 0., 600., "attack")?;
            check(*decay, 0., 600., "decay")?;
        }
        Component::Bd(nodes::Bd { param })
        | Component::Sn(nodes::Sn { param })
        | Component::Hh(nodes::Hh { param }) => signal(param, 0., 600., "decay")?,
        Component::Pulse(nodes::Pulse { frequency, width }) => {
            signal(
                frequency,
                -(sr as f32) * 0.499,
                sr as f32 * 0.499,
                "frequency",
            )?;
            signal(width, 0.001, 0.999, "pulse width")?;
        }
        Component::Wavetable(nodes::Wavetable {
            frequency,
            position,
            ..
        }) => {
            signal(
                frequency,
                -(sr as f32) * 0.499,
                sr as f32 * 0.499,
                "frequency",
            )?;
            signal(position, 0., 1., "wavetable position")?;
        }
        Component::Points(nodes::Points {
            points,
            span,
            is_looping,
        }) => {
            check(*span, f32::MIN_POSITIVE, 86400., "curve span")?;
            if points.is_empty() {
                return Err(EngineError::InvalidParameter("empty curve".into()));
            }
            let mut last = -1.;
            for (time, value) in points {
                check(time.bar, 0., 86400., "curve bar")?;
                check(*value, -1e6, 1e6, "curve value")?;
                let offset = match time.time {
                    Some(nodes::Duration::Seconds(s)) => s,
                    Some(nodes::Duration::Milliseconds(ms)) => ms / 1000.,
                    Some(nodes::Duration::Bar(b)) => b * 240. / bpm * span,
                    None => 0.,
                };
                let seconds = time.bar * 240.0 / bpm * span + offset;
                check(seconds, 0., 86400., "curve timestamp")?;
                if seconds < last {
                    return Err(EngineError::InvalidParameter(
                        "curve timestamps must be ordered".into(),
                    ));
                }
                if *is_looping && seconds > 240.0 / bpm * span {
                    return Err(EngineError::InvalidParameter(
                        "loop curve timestamp exceeds its period".into(),
                    ));
                }
                last = seconds;
            }
        }
        Component::Plate(nodes::Plate { mix }) => check(*mix, 0., 1., "plate mix")?,
        Component::Rhpf(nodes::Rhpf { qvalue, .. }) | Component::Lpf(nodes::Lpf { qvalue, .. }) => {
            check(*qvalue, 0.01, 100., "filter Q")?
        }
        Component::Delayms(nodes::Delayms { param }) => {
            signal(param, 0., 10000., "delay milliseconds")?
        }
        Component::Pan(nodes::Pan { param }) => signal(param, -1., 1., "pan")?,
        Component::ConstSig(nodes::ConstSig { value }) => check(*value, -1e12, 1e12, "signal")?,
        Component::Imp(nodes::Imp { param }) => {
            signal(param, f32::MIN_POSITIVE, sr as f32, "impulse frequency")?
        }
        _ => {}
    }
    Ok(())
}
