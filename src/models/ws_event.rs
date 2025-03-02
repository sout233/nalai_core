use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WSEvent {
    pub event_type: WSEventType,
    pub data: Value,
}

impl WSEvent {
    pub fn new(event_type: WSEventType, data: Value) -> Self {
        Self { event_type, data }
    }

    pub fn to_value(self) -> Value {
        json!(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WSEventType {
    DownloadProgressChanged,
    DownloadStatusChanged,
    Raw,
}
