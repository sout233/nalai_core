use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct NalaiResult {
    success: bool,
    code: u16,
    msg: Option<String>,
    data: Value,
}

impl NalaiResult {
    pub fn new(code: StatusCode,msg: Option<&str>, data: Value) -> Self {
        Self {
            success: code.is_success(),
            code: code.as_u16(),
            msg: msg.map(|m| m.to_string()),
            data,
        }
    }
}