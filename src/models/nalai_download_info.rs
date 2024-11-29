use std::{num::NonZero, time::SystemTime};

use serde::{Deserialize, Serialize};
use crate::models::status_wrapper::StatusWrapper;
use crate::models::chunk_wrapper::ChunkWrapper;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct NalaiDownloadInfo {
    pub(crate) downloaded_bytes: u64,
    pub(crate) total_size: NonZero<u64>,
    pub(crate) file_name: String,
    pub(crate) url: String,
    pub(crate) status: StatusWrapper,
    pub(crate) speed: u64,
    pub(crate) save_dir: String,
    pub(crate) create_time: SystemTime,
    pub(crate) chunks: Vec<ChunkWrapper>
}

impl Default for NalaiDownloadInfo {
    fn default() -> Self {
        Self {
            downloaded_bytes: Default::default(),
            total_size: NonZero::new(1).unwrap(),
            file_name: Default::default(),
            url: Default::default(),
            status: StatusWrapper::NoStart,
            speed: Default::default(),
            save_dir: Default::default(),
            create_time: SystemTime::now(),
            chunks: Default::default()
        }
    }
}

