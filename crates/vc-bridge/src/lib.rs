//! Maps continuous latent vectors into [`vc_ir::Module`] (planned: ONNX / learned decoder).
//!
//! Default build is **stub-only**; enable feature `onnx` when wiring ONNX Runtime.
//!
//! Design notes and contracts (`z` layout, placeholder ONNX I/O names, export outline, failure
//! modes): repository **`docs/DECODER_ROADMAP.md`**.

#![forbid(unsafe_code)]

mod decoder;

#[cfg(feature = "onnx")]
mod onnx;

pub use decoder::{GoldenLatentDecoder, LatentDecoder, StubLatentDecoder, EMBEDDING_DIM};

#[cfg(feature = "onnx")]
pub use onnx::{OrtLatentDecoder, DECODER_ONNX_INPUT_Z, DECODER_ONNX_OUTPUT_IR_JSON};
