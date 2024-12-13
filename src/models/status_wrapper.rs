use std::fmt::Display;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub(crate) enum StatusWrapperKind {
    NoStart,
    Running,
    Pending,
    Error,
    Finished,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub(crate) struct StatusWrapper {
    pub kind: StatusWrapperKind,
    pub message: Option<String>,
}

impl StatusWrapper {
    pub fn new(kind: StatusWrapperKind) -> Self {
        Self {
            kind,
            message: None,
        }
    }

    pub fn with_message<T>(self, message: T) -> Self
    where
        T: Into<String>,
    {
        Self {
            kind: self.kind,
            message: Some(message.into()),
        }
    }
}

impl Default for StatusWrapper {
    fn default() -> Self {
        Self::new(StatusWrapperKind::NoStart)
    }
}

impl Display for StatusWrapperKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StatusWrapperKind::NoStart => write!(f, "NoStart"),
            StatusWrapperKind::Running => write!(f, "Running"),
            StatusWrapperKind::Pending => write!(f, "Pending"),
            StatusWrapperKind::Error => write!(f, "Error"),
            StatusWrapperKind::Finished => write!(f, "Finished"),
        }
    }
}
