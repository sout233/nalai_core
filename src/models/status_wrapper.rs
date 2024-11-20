use std::fmt::Display;

use serde::{Deserialize, Serialize};


#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) enum StatusWrapper {
    NoStart,
    Running,
    Pending,
    Error,
    Finished,
}

impl Display for StatusWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StatusWrapper::NoStart => write!(f, "NoStart"),
            StatusWrapper::Running => write!(f, "Running"),
            StatusWrapper::Pending => write!(f, "Pending"),
            StatusWrapper::Error => write!(f, "Error"),
            StatusWrapper::Finished => write!(f, "Finished"),
        }
    }
}