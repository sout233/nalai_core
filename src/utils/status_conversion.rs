use http_downloader::{status_tracker::DownloaderStatus, DownloadError, DownloadingEndCause};
use crate::models::status_wrapper::StatusWrapper;

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
            DownloaderStatus::NoStart => StatusWrapper::NoStart,
            DownloaderStatus::Running => StatusWrapper::Running,
            DownloaderStatus::Pending(_) => StatusWrapper::Pending,
            DownloaderStatus::Error(_e) => StatusWrapper::Error,
            DownloaderStatus::Finished => StatusWrapper::Finished,
        }
    }
}

impl IntoStatusWrapper for DownloadingEndCause {
    fn into_status_wrapper(self) -> StatusWrapper {
        match self {
            DownloadingEndCause::DownloadFinished => StatusWrapper::Finished,
            DownloadingEndCause::Cancelled => StatusWrapper::NoStart,
        }
    }
}

impl IntoStatusWrapper for DownloadError {
    fn into_status_wrapper(self) -> StatusWrapper {
        match self {
            _ => StatusWrapper::Error,
        }
    }
}

pub(crate) fn convert_status<T>(status: T) -> StatusWrapper
where
    T: IntoStatusWrapper,
{
    status.into_status_wrapper()
}