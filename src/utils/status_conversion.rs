use crate::models::status_wrapper::{StatusWrapper, StatusWrapperKind};
use http_downloader::{status_tracker::DownloaderStatus, DownloadError, DownloadingEndCause};

pub trait IntoStatusWrapper {
    fn into_status_wrapper(self) -> StatusWrapper;
}

// 创建一个新的包装类型
pub(crate) struct DownloaderStatusWrapper(DownloaderStatus);

// 为包装类型实现 From<DownloaderStatus>，以便于构造
impl From<DownloaderStatus> for DownloaderStatusWrapper {
    fn from(status: DownloaderStatus) -> Self {
        DownloaderStatusWrapper(status)
    }
}

impl IntoStatusWrapper for DownloaderStatusWrapper {
    fn into_status_wrapper(self) -> StatusWrapper {
        match self.0 {
            DownloaderStatus::NoStart => StatusWrapper::new(StatusWrapperKind::NoStart),
            DownloaderStatus::Running => StatusWrapper::new(StatusWrapperKind::Running),
            DownloaderStatus::Pending(_) => StatusWrapper::new(StatusWrapperKind::Pending),
            DownloaderStatus::Error(e) => {
                StatusWrapper::new(StatusWrapperKind::Error).with_message(e)
            }
            DownloaderStatus::Finished => StatusWrapper::new(StatusWrapperKind::Finished),
        }
    }
}

impl IntoStatusWrapper for DownloadingEndCause {
    fn into_status_wrapper(self) -> StatusWrapper {
        match self {
            DownloadingEndCause::DownloadFinished => {
                StatusWrapper::new(StatusWrapperKind::Finished)
            }
            DownloadingEndCause::Cancelled => StatusWrapper::new(StatusWrapperKind::NoStart),
        }
    }
}

impl IntoStatusWrapper for DownloadError {
    fn into_status_wrapper(self) -> StatusWrapper {
        match self {
            DownloadError::Other(error) => {
                StatusWrapper::new(StatusWrapperKind::Error).with_message(error.to_string())
            }
            DownloadError::ArchiveDataLoadError(error) => {
                StatusWrapper::new(StatusWrapperKind::Error).with_message(error.to_string())
            }
            DownloadError::IoError(error) => {
                StatusWrapper::new(StatusWrapperKind::Error).with_message(error.to_string())
            }
            DownloadError::JoinError(join_error) => {
                StatusWrapper::new(StatusWrapperKind::Error).with_message(join_error.to_string())
            }
            DownloadError::ChunkRemoveFailed(_) => StatusWrapper::new(StatusWrapperKind::Error)
                .with_message("Failed to remove chunk file"),
            DownloadError::DownloadingChunkRemoveFailed(_) => {
                StatusWrapper::new(StatusWrapperKind::Error)
                    .with_message("Failed to remove downloading chunk file")
            }
            DownloadError::HttpRequestFailed(error) => {
                StatusWrapper::new(StatusWrapperKind::Error).with_message(error.to_string())
            }
            DownloadError::HttpRequestResponseInvalid(cause, response) => {
                StatusWrapper::new(StatusWrapperKind::Error).with_message(format!(
                    "Invalid http response: {}, response: {}",
                    format!("{:?}", cause),
                    format!("{:?}", response)
                ))
            }
            DownloadError::ServerFileAlreadyChanged => StatusWrapper::new(StatusWrapperKind::Error)
                .with_message("Server file already changed"),
            DownloadError::RedirectionTimesTooMany => StatusWrapper::new(StatusWrapperKind::Error)
                .with_message("Redirection times too many"),
        }
    }
}

pub(crate) fn convert_status<T>(status: T) -> StatusWrapper
where
    T: IntoStatusWrapper,
{
    status.into_status_wrapper()
}
