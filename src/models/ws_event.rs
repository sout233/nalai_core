use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WSEvent {
    pub event_type: WSEventType,
    pub id: Option<String>,
    pub data: Value,
}

impl WSEvent {
    pub fn new(event_type: WSEventType,id: Option<String>, data: Value) -> Self {
        Self { event_type, id, data }
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
