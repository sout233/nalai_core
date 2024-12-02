use std::{collections::HashMap, sync::Arc};

use http_downloader::ChunkItem;
use serde::{Deserialize, Serialize};

#[derive(Debug,Serialize,Deserialize,Clone)]
pub(crate) struct ChunkWrapper {
    pub index: usize,
    pub size: u64,
    pub downloaded_bytes: u64,
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

pub fn merge_chunks(original_chunks: Vec<ChunkWrapper>, new_chunks: Vec<ChunkWrapper>) -> Vec<ChunkWrapper> {
    let mut original_map: HashMap<usize, ChunkWrapper> = original_chunks.into_iter().map(|c| (c.index, c)).collect();

    for chunk in new_chunks {
        if let Some(original_chunk) = original_map.get_mut(&chunk.index) {
            original_chunk.downloaded_bytes = chunk.downloaded_bytes;
            original_chunk.size = chunk.size;
        } else {
            original_map.insert(chunk.index, chunk);
        }
    }

    let mut merged_chunks: Vec<ChunkWrapper> = original_map.into_iter().map(|(_, c)| c).collect();
    merged_chunks.sort_by_key(|c| c.index);

    merged_chunks
}