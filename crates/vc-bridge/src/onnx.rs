//! ONNX Runtime (`ort`) integration: load a decoder graph, validate frozen I/O contracts, run
//! inference, and materialize [`vc_ir::Module`] from the **`program_ir_json`** output tensor.
//!
//! ## Fail-closed boundary
//!
//! [`OrtLatentDecoder::decode_z`](OrtLatentDecoder::decode_z) returns [`anyhow::Error`] on invalid
//! `z`, missing model, load failures, contract mismatch, inference failures, oversized payloads,
//! invalid UTF-8 / JSON, or IR that fails `validate_module`.
//!
//! ## Frozen contracts (Phase 0 + tensor→IR)
//!
//! - **Input:** [`DECODER_ONNX_INPUT_Z`] — `f32` tensor with logical shape **`[1, D]`** or **`[D]`**
//!   (`D == EXPECTED_Z_LEN`). Model metadata is checked at session load when ORT exposes ranks/dims.
//!   Inference always feeds **`[1, D]`** row-major flattened `z`.
//! - **Output:** [`DECODER_ONNX_OUTPUT_IR_JSON`] — **`uint8`** rank-1 tensor whose bytes are **UTF-8
//!   JSON** for a single [`Module`] (same shape as `.vcir` on disk). Parsed with
//!   [`Module::parse_json_slice`](vc_ir::Module::parse_json_slice) then `validate_module`.

use crate::decoder::LatentDecoder;
use anyhow::{bail, Context, Result};
use ort::session::Session;
use ort::tensor::TensorElementType;
use ort::value::{Tensor, ValueType};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use vc_ir::Module;

/// Upper bound on **`program_ir_json`** raw bytes accepted from ONNX (defense in depth).
const IR_JSON_ONNX_MAX_BYTES: usize = 16 * 1024 * 1024;

/// ONNX graph input name for the flattened latent **`z`** tensor (batch × `D` or `D`).
///
/// Documented in workspace `docs/DECODER_ROADMAP.md`.
pub const DECODER_ONNX_INPUT_Z: &str = "z";

/// ONNX graph output name carrying **Program IR JSON** as a **`uint8`** tensor (flat UTF-8 bytes).
///
/// Training exports must expose this tensor; see `docs/DECODER_ROADMAP.md`.
pub const DECODER_ONNX_OUTPUT_IR_JSON: &str = "program_ir_json";

/// Decoder backed by ONNX Runtime: lazy [`Session`], contract validation, inference, IR extraction.
pub struct OrtLatentDecoder {
    model_path: Option<PathBuf>,
    session: Mutex<Option<Session>>,
}

impl OrtLatentDecoder {
    pub fn new() -> Self {
        Self {
            model_path: None,
            session: Mutex::new(None),
        }
    }

    pub fn with_model_path<P: AsRef<Path>>(path: P) -> Self {
        Self {
            model_path: Some(path.as_ref().to_path_buf()),
            session: Mutex::new(None),
        }
    }

    /// Path configured by [`Self::with_model_path`](OrtLatentDecoder::with_model_path), if any.
    pub fn model_path(&self) -> Option<&Path> {
        self.model_path.as_deref()
    }
}

impl Default for OrtLatentDecoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Accepts ORT-declared shapes `[D]`, `[1, D]`, or dynamic `-1` placeholders with rank 1–2.
fn z_input_shape_matches_contract(shape: &[i64], expected_len: usize) -> Result<()> {
    let expected_len = i64::try_from(expected_len).context("expected z length does not fit i64")?;
    match shape {
        [d] => {
            if *d > 0 && *d != expected_len {
                bail!(
                    "decoder input {:?} shape {:?} must have {} elements (got fixed dim {})",
                    DECODER_ONNX_INPUT_Z,
                    shape,
                    expected_len,
                    d
                );
            }
        }
        [batch, d] => {
            if *batch > 0 && *batch != 1 {
                bail!(
                    "decoder input {:?} batch dimension must be 1 or dynamic (-1), got {:?}",
                    DECODER_ONNX_INPUT_Z,
                    shape
                );
            }
            if *d > 0 && *d != expected_len {
                bail!(
                    "decoder input {:?} shape {:?} must end with latent dim {} (got {})",
                    DECODER_ONNX_INPUT_Z,
                    shape,
                    expected_len,
                    d
                );
            }
        }
        _ => {
            bail!(
                "decoder input {:?} must be rank-1 `[{}]` or rank-2 `[1, {}]` (got shape {:?})",
                DECODER_ONNX_INPUT_Z,
                expected_len,
                expected_len,
                shape
            );
        }
    }
    Ok(())
}

fn validate_z_input_contract(session: &Session) -> Result<()> {
    let expected_len = <OrtLatentDecoder as LatentDecoder>::EXPECTED_Z_LEN;
    let input = session
        .inputs
        .iter()
        .find(|i| i.name == DECODER_ONNX_INPUT_Z)
        .with_context(|| {
            format!(
                "decoder ONNX model must declare an input tensor named {:?}; got {:?}",
                DECODER_ONNX_INPUT_Z,
                session
                    .inputs
                    .iter()
                    .map(|i| i.name.as_str())
                    .collect::<Vec<_>>()
            )
        })?;

    match &input.input_type {
        ValueType::Tensor { ty, shape, .. } => {
            anyhow::ensure!(
                *ty == TensorElementType::Float32,
                "decoder input {:?} must be an f32 tensor (got element type {:?})",
                DECODER_ONNX_INPUT_Z,
                ty
            );
            z_input_shape_matches_contract(shape, expected_len)?;
        }
        other => {
            bail!(
                "decoder input {:?} must be a tensor (got {:?})",
                DECODER_ONNX_INPUT_Z,
                other
            );
        }
    }
    Ok(())
}

fn validate_ir_json_output_contract(session: &Session) -> Result<()> {
    let output = session
        .outputs
        .iter()
        .find(|o| o.name == DECODER_ONNX_OUTPUT_IR_JSON)
        .with_context(|| {
            format!(
                "decoder ONNX model must declare output tensor {:?} (UTF-8 Program IR JSON); got {:?}",
                DECODER_ONNX_OUTPUT_IR_JSON,
                session
                    .outputs
                    .iter()
                    .map(|o| o.name.as_str())
                    .collect::<Vec<_>>()
            )
        })?;

    match &output.output_type {
        ValueType::Tensor { ty, .. } => {
            anyhow::ensure!(
                *ty == TensorElementType::Uint8,
                "decoder output {:?} must be a uint8 tensor (got element type {:?})",
                DECODER_ONNX_OUTPUT_IR_JSON,
                ty
            );
        }
        other => {
            bail!(
                "decoder output {:?} must be a tensor (got {:?})",
                DECODER_ONNX_OUTPUT_IR_JSON,
                other
            );
        }
    }
    Ok(())
}

fn module_from_ir_json_output(outputs: &ort::session::SessionOutputs<'_>) -> Result<Module> {
    let value = outputs.get(DECODER_ONNX_OUTPUT_IR_JSON).with_context(|| {
        format!(
            "ONNX run missing output {:?} (expected UTF-8 Program IR JSON as uint8 tensor)",
            DECODER_ONNX_OUTPUT_IR_JSON
        )
    })?;

    let (_shape, raw) = value.try_extract_tensor::<u8>().with_context(|| {
        format!(
            "output {:?} must be a uint8 tensor",
            DECODER_ONNX_OUTPUT_IR_JSON
        )
    })?;

    anyhow::ensure!(
        raw.len() <= IR_JSON_ONNX_MAX_BYTES,
        "{:?} payload too large: {} bytes (max {})",
        DECODER_ONNX_OUTPUT_IR_JSON,
        raw.len(),
        IR_JSON_ONNX_MAX_BYTES
    );

    let module = Module::parse_json_slice(raw)
        .with_context(|| format!("{:?}: invalid Program IR JSON", DECODER_ONNX_OUTPUT_IR_JSON))?;

    vc_ir::validate_module(&module).with_context(|| {
        format!(
            "{:?}: IR failed validate_module",
            DECODER_ONNX_OUTPUT_IR_JSON
        )
    })?;

    Ok(module)
}

impl LatentDecoder for OrtLatentDecoder {
    fn decode_z(&self, z: &[f32]) -> Result<Module> {
        if z.len() != Self::EXPECTED_Z_LEN {
            bail!(
                "expected z.len() == {}, got {}",
                Self::EXPECTED_Z_LEN,
                z.len()
            );
        }

        let Some(path) = self.model_path.as_ref() else {
            bail!(
                "OrtLatentDecoder: no ONNX model path configured; pass `--onnx-model` / \
                 OrtLatentDecoder::with_model_path"
            );
        };

        if !path.is_file() {
            bail!(
                "OrtLatentDecoder: ONNX model path is not a file: {}",
                path.display()
            );
        }

        let mut slot = self
            .session
            .lock()
            .map_err(|_| anyhow::anyhow!("OrtLatentDecoder: internal mutex poisoned"))?;

        if slot.is_none() {
            let session = Session::builder()
                .context("OrtLatentDecoder: Session::builder")?
                .commit_from_file(path)
                .map_err(|e| {
                    anyhow::anyhow!(
                        "OrtLatentDecoder: failed to load ONNX {}: {:#}",
                        path.display(),
                        e
                    )
                })?;
            validate_z_input_contract(&session)?;
            validate_ir_json_output_contract(&session)?;
            *slot = Some(session);
        }

        let session = match slot.as_mut() {
            Some(s) => s,
            None => bail!("OrtLatentDecoder: internal error (empty ONNX session slot after load)"),
        };

        let tensor = Tensor::from_array(([1usize, Self::EXPECTED_Z_LEN], z.to_vec()))
            .context("OrtLatentDecoder: failed to build input tensor")?;

        let outputs = session
            .run(ort::inputs! { DECODER_ONNX_INPUT_Z => tensor })
            .context("OrtLatentDecoder: ONNX Runtime inference failed")?;

        module_from_ir_json_output(&outputs)
    }
}

#[cfg(all(test, feature = "onnx"))]
mod tests {
    use super::*;
    use crate::decoder::{GoldenLatentDecoder, LatentDecoder, EMBEDDING_DIM};
    use std::path::PathBuf;

    fn decoder_identity_fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmarks/fixtures/decoder_identity_z.onnx")
    }

    fn decoder_z_switch_fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmarks/fixtures/decoder_z_switch.onnx")
    }

    fn golden_mul_module() -> Module {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/programs/mul.vcir");
        let bytes = std::fs::read(&path).unwrap_or_else(|e| {
            panic!("read {}: {e}", path.display());
        });
        let module = Module::parse_json_slice(&bytes)
            .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
        vc_ir::validate_module(&module).expect("validate mul.vcir");
        module
    }

    #[test]
    fn ort_expected_z_len_matches_workspace_contract() {
        assert_eq!(
            <OrtLatentDecoder as LatentDecoder>::EXPECTED_Z_LEN,
            EMBEDDING_DIM
        );
    }

    #[test]
    fn ort_rejects_wrong_z_len_without_loading_session() {
        let dec = OrtLatentDecoder::with_model_path("/nonexistent/model.onnx");
        let err = dec.decode_z(&[0.0f32; 4]).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("expected z.len()"),
            "expected dimension error, got: {msg}"
        );
        assert!(
            !msg.contains("model.onnx"),
            "wrong-len path must not mention model path or session: {msg}"
        );
    }

    #[test]
    fn ort_fails_closed_without_model_path() {
        let dec = OrtLatentDecoder::new();
        let z = vec![0.0f32; EMBEDDING_DIM];
        let err = dec.decode_z(&z).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("no ONNX model path"), "got: {msg}");
    }

    #[test]
    fn ort_rejects_missing_model_file() {
        let dec = OrtLatentDecoder::with_model_path("/this/path/should/not/exist/model.onnx");
        let err = dec.decode_z(&vec![0.0f32; EMBEDDING_DIM]).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not a file") || msg.contains("No such file"),
            "got: {msg}"
        );
    }

    #[test]
    fn ort_identity_fixture_decodes_to_same_ir_as_golden() {
        let path = decoder_identity_fixture();
        assert!(
            path.is_file(),
            "missing fixture {}; generate via scripts/gen_decoder_identity_fixture_onnx.py",
            path.display()
        );
        let z = vec![0.0f32; EMBEDDING_DIM];
        let golden = GoldenLatentDecoder.decode_z(&z).expect("golden");
        let onnx_module = OrtLatentDecoder::with_model_path(&path)
            .decode_z(&z)
            .expect("onnx fixture decode");
        assert_eq!(onnx_module, golden);
    }

    #[test]
    fn ort_z_switch_fixture_nonneg_z0_emits_add_ir() {
        let path = decoder_z_switch_fixture();
        assert!(
            path.is_file(),
            "missing fixture {}; generate via scripts/gen_decoder_z_switch_fixture_onnx.py",
            path.display()
        );
        let z = vec![0.0f32; EMBEDDING_DIM];
        let golden = GoldenLatentDecoder.decode_z(&z).expect("golden add");
        let onnx_module = OrtLatentDecoder::with_model_path(&path)
            .decode_z(&z)
            .expect("z_switch nonneg decode");
        assert_eq!(onnx_module, golden);
    }

    #[test]
    fn ort_z_switch_fixture_neg_z0_emits_mul_ir() {
        let path = decoder_z_switch_fixture();
        assert!(
            path.is_file(),
            "missing fixture {}; generate via scripts/gen_decoder_z_switch_fixture_onnx.py",
            path.display()
        );
        let mut z = vec![0.0f32; EMBEDDING_DIM];
        z[0] = -1.0;
        let golden_mul = golden_mul_module();
        let onnx_module = OrtLatentDecoder::with_model_path(&path)
            .decode_z(&z)
            .expect("z_switch neg decode");
        assert_eq!(onnx_module, golden_mul);
        assert_ne!(
            onnx_module,
            GoldenLatentDecoder
                .decode_z(&vec![0.0; EMBEDDING_DIM])
                .unwrap()
        );
    }
}
