use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct NalaiResult {
    success: bool,
    code: String,
    data: Value,
}

impl NalaiResult {
    pub fn new(success: bool, code: StatusCode, data: Value) -> Self {
        Self {
            success,
            code: code.to_string(),
            data,
        }
    }
}