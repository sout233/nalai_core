use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WsQuery {
    pub kind: String,
    pub data: Option<String>,
}