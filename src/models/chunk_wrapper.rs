use std::sync::Arc;

use http_downloader::ChunkItem;
use serde::{Deserialize, Serialize};

#[derive(Debug,Serialize,Deserialize,Clone)]
pub(crate) struct ChunkWrapper {
    index: usize,
    size: u64,
    downloaded_bytes: u64,
}

impl ChunkWrapper {
    /// Creates a new [`ChunkWrapper`].
    #[allow(dead_code)]
    pub(crate) fn new(index: usize, size: u64, downloaded_bytes: u64) -> Self {
        Self {
            index,
            size,
            downloaded_bytes
        }
    }

    pub(crate) fn from(chunk:Arc<ChunkItem>)-> Self {
        Self {
            index: chunk.chunk_info.index,
            size: chunk.chunk_info.range.len(),
            downloaded_bytes: chunk.downloaded_len.load(std::sync::atomic::Ordering::Relaxed)
        }
    }
}