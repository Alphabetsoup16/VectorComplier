use serde::{Deserialize, Serialize};

/// One I/O case, matching benchmark manifest `cases` entries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecCase {
    pub args: Vec<i32>,
    pub expect_i32: i32,
}

/// Behavioral spec for refinement (manifest-style cases only).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spec {
    pub cases: Vec<SpecCase>,
}

impl Spec {
    pub fn from_json_slice(bytes: &[u8]) -> serde_json::Result<Self> {
        serde_json::from_slice(bytes)
    }
}
