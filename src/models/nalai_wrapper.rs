use std::sync::Arc;

use http_downloader::ExtendedHttpFileDownloader;
use tokio::sync::Mutex;
use crate::models::nalai_download_info::NalaiDownloadInfo;

#[derive(Clone)]
pub(crate) struct NalaiWrapper {
    pub(crate) downloader: Option<Arc<Mutex<ExtendedHttpFileDownloader>>>,
    pub(crate) info: NalaiDownloadInfo,
}