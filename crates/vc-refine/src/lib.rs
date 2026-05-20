//! Verifier-driven refinement over Program IR (random search in v0).

mod check;
mod random;
mod refiner;
mod spec;

pub use random::{wrong_sub_add_signature, RandomIrRefiner};
pub use refiner::ProgramRefiner;
pub use spec::{Spec, SpecCase};

#[cfg(test)]
mod tests {
    use super::*;
    use vc_ir::{Func, FuncSig, Instr, Module, ValType, PROGRAM_IR_VERSION};

    fn add_spec() -> Spec {
        Spec {
            cases: vec![
                SpecCase {
                    args: vec![1, 2],
                    expect_i32: 3,
                },
                SpecCase {
                    args: vec![40, 2],
                    expect_i32: 42,
                },
                SpecCase {
                    args: vec![-1, 1],
                    expect_i32: 0,
                },
            ],
        }
    }

    #[test]
    fn spec_roundtrip_json() {
        let spec = add_spec();
        let raw = serde_json::to_vec(&spec).expect("serialize");
        let back: Spec = Spec::from_json_slice(&raw).expect("parse");
        assert_eq!(back, spec);
    }

    #[test]
    fn random_refiner_finds_add_from_sub() {
        let spec = add_spec();
        let initial = wrong_sub_add_signature();
        let refiner = RandomIrRefiner::new(0xADD5EED);
        let refined = refiner
            .refine(&initial, &spec, 50_000, 256)
            .expect("should find add program");
        let mut cache = crate::check::ModuleSpecCache::new();
        cache
            .satisfies(&refined, &spec, 50_000, None)
            .expect("refined module passes spec");
        assert!(
            refined.func.body.windows(2).any(|w| {
                matches!(
                    (&w[0], &w[1]),
                    (&Instr::LocalGet { index: 0 }, &Instr::LocalGet { index: 1 })
                        | (&Instr::LocalGet { index: 1 }, &Instr::LocalGet { index: 0 })
                )
            }),
            "expected i32 add of both params"
        );
        assert!(refined.func.body.contains(&Instr::I32Add));
    }

    #[test]
    fn refine_returns_initial_when_already_correct() {
        let spec = add_spec();
        let initial = Module {
            program_ir_version: PROGRAM_IR_VERSION,
            export_name: "run".into(),
            func: Func {
                sig: FuncSig {
                    params: vec![ValType::I32, ValType::I32],
                    results: vec![ValType::I32],
                },
                locals: vec![],
                body: vec![
                    Instr::LocalGet { index: 0 },
                    Instr::LocalGet { index: 1 },
                    Instr::I32Add,
                    Instr::Return,
                ],
            },
        };
        let refiner = RandomIrRefiner::new(1);
        let out = refiner
            .refine(&initial, &spec, 50_000, 10)
            .expect("noop refine");
        assert_eq!(out, initial);
    }
}
