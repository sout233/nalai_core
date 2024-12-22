use std::{collections::HashMap, num::NonZero, time::SystemTime};

use serde::{Deserialize, Serialize};
use crate::models::chunk_wrapper::ChunkWrapper;
use super::status_wrapper::StatusWrapper;

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
    pub(crate) chunks: Vec<ChunkWrapper>,
    pub(crate) headers: HashMap<String, String>,
}

impl Default for NalaiDownloadInfo {
    fn default() -> Self {
        Self {
            downloaded_bytes: Default::default(),
            total_size: NonZero::new(1).unwrap(),
            file_name: Default::default(),
            url: Default::default(),
            status: StatusWrapper::default(),
            speed: Default::default(),
            save_dir: Default::default(),
            create_time: SystemTime::now(),
            chunks: Default::default(),
            headers: Default::default(),
        }
    }
}

