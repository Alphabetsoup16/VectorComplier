use anyhow::{bail, Result};
use vc_ir::{validate_module, Func, FuncSig, Instr, Module, ValType, PROGRAM_IR_VERSION};

/// Fixed embedding dimension for decoder contracts (adjust when training exports change).
///
/// Implementations default [`LatentDecoder::EXPECTED_Z_LEN`] to this value unless a specialized
/// decoder exports a different latent width.
pub const EMBEDDING_DIM: usize = 256;

/// Decode a flattened latent vector `z` into Program IR (`vc_ir::Module`).
///
/// # Tensor layout contract
///
/// - **Element type:** `f32`.
/// - **Slice layout:** callers pass **row-major flattened** coefficients as `&[f32]` whose length
///   must equal [`EXPECTED_Z_LEN`](LatentDecoder::EXPECTED_Z_LEN). Logical ONNX shapes such as
///   `[1, D]` or `[D]` must flatten to that same element count.
/// - **Normalization:** undefined here — training/export must document scaling (if any); ONNX and
///   Rust paths must agree.
///
/// Implementations must **fail closed**: invalid lengths and unsupported configurations return
/// [`anyhow::Error`] without panicking on normal caller-controlled inputs.
///
/// Repository-facing roadmap (frozen ONNX **`z`** / **`program_ir_json`** tensors, export pipeline, failure modes):
/// workspace `docs/DECODER_ROADMAP.md`.
pub trait LatentDecoder: Send + Sync {
    /// Expected number of `f32` elements in `z` for [`decode_z`](LatentDecoder::decode_z).
    ///
    /// Defaults to [`EMBEDDING_DIM`]; override only when the exported decoder uses a different
    /// latent width (keep docs and ONNX metadata in sync).
    const EXPECTED_Z_LEN: usize = EMBEDDING_DIM;

    fn decode_z(&self, z: &[f32]) -> Result<Module>;
}

/// Always fails; used until a trained decoder or ONNX session is wired.
pub struct StubLatentDecoder;

impl LatentDecoder for StubLatentDecoder {
    fn decode_z(&self, z: &[f32]) -> Result<Module> {
        if z.len() != Self::EXPECTED_Z_LEN {
            bail!(
                "expected z.len() == {}, got {}",
                Self::EXPECTED_Z_LEN,
                z.len()
            );
        }
        bail!("StubLatentDecoder: no learned decoder wired (enable `onnx` when ORT is available)")
    }
}

/// Deterministic decoder for **demos and CI**: ignores `z` values (after length check) and returns a
/// fixed valid Program IR module equivalent to `benchmarks/programs/add.vcir`.
///
/// This is **not** a learned mapping — use [`StubLatentDecoder`] until ONNX/training is wired.
pub struct GoldenLatentDecoder;

impl LatentDecoder for GoldenLatentDecoder {
    fn decode_z(&self, z: &[f32]) -> Result<Module> {
        if z.len() != Self::EXPECTED_Z_LEN {
            bail!(
                "expected z.len() == {}, got {}",
                Self::EXPECTED_Z_LEN,
                z.len()
            );
        }
        let module = Module {
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
        validate_module(&module)?;
        Ok(module)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_returns_valid_add_ir() {
        let z = vec![0.0f32; EMBEDDING_DIM];
        let m = GoldenLatentDecoder.decode_z(&z).expect("golden decode");
        vc_ir::validate_module(&m).expect("validate golden IR");
        assert_eq!(m.export_name, "run");
    }

    #[test]
    fn latent_decoder_default_expected_z_len_matches_embedding_dim() {
        assert_eq!(
            <StubLatentDecoder as LatentDecoder>::EXPECTED_Z_LEN,
            EMBEDDING_DIM
        );
    }

    #[test]
    fn stub_rejects_wrong_embedding_dim() {
        let err = StubLatentDecoder.decode_z(&[0.0; 4]).unwrap_err();
        assert!(
            err.to_string()
                .contains(&format!("expected z.len() == {}", EMBEDDING_DIM)),
            "{err}"
        );
    }

    #[test]
    fn stub_fails_with_correct_dim() {
        let z = vec![0.0f32; EMBEDDING_DIM];
        let err = StubLatentDecoder.decode_z(&z).unwrap_err();
        assert!(err.to_string().contains("StubLatentDecoder"), "{err}");
    }
}
