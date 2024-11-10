use base64::{engine::general_purpose, Engine};
use http_downloader::{
    breakpoint_resume::DownloadBreakpointResumeExtension,
    bson_file_archiver::{ArchiveFilePath, BsonFileArchiverBuilder},
    speed_limiter::DownloadSpeedLimiterExtension,
    speed_tracker::DownloadSpeedTrackerExtension,
    status_tracker::{DownloadStatusTrackerExtension, DownloaderStatus},
    ExtendedHttpFileDownloader, HttpDownloaderBuilder,
};
use once_cell::sync::Lazy;
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::HashMap,
    num::{NonZero, NonZeroU8, NonZeroUsize},
    path::PathBuf,
    sync::Arc,
    thread,
    time::Duration,
};
use tokio::sync::Mutex;
use tracing::info;
use url::Url;

static GLOBAL_WRAPPERS: Lazy<Mutex<HashMap<String, NalaiWrapper>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Clone)]
struct NalaiWrapper {
    downloader: Arc<Mutex<ExtendedHttpFileDownloader>>,
    info: NalaiDownloadInfo,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct NalaiDownloadInfo {
    downloaded_bytes: u64,
    total_size: NonZero<u64>,
    file_name: String,
    url: String,
    status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
enum StatusWrapper{
    NoStart,
    Running,
    Pending(String),
    Error(String),
    Finished,
}

#[handler]
async fn start_download(req: &mut Request, res: &mut Response) {
    let save_dir = req.query::<String>("save_dir").unwrap_or_default();
    let save_dir = PathBuf::from(save_dir);
    let url = req.query::<String>("url").unwrap_or_default();
    let url = Url::parse(&url).unwrap();

    let (mut downloader, (status_state, speed_state, speed_limiter, ..)) =
        HttpDownloaderBuilder::new(url, save_dir)
            .chunk_size(NonZeroUsize::new(1024 * 1024 * 10).unwrap()) // 块大小
            .download_connection_count(NonZeroU8::new(3).unwrap())
            .build((
                // 下载状态追踪扩展
                // by cargo feature "status-tracker" enable
                DownloadStatusTrackerExtension { log: true },
                // 下载速度追踪扩展
                // by cargo feature "speed-tracker" enable
                DownloadSpeedTrackerExtension { log: true },
                // 下载速度限制扩展，
                // by cargo feature "speed-limiter" enable
                DownloadSpeedLimiterExtension::new(None),
                // 断点续传扩展，
                // by cargo feature "breakpoint-resume" enable
                DownloadBreakpointResumeExtension {
                    // BsonFileArchiver by cargo feature "bson-file-archiver" enable
                    download_archiver_builder: BsonFileArchiverBuilder::new(
                        ArchiveFilePath::Suffix("bson".to_string()),
                    ),
                },
            ));

    let file_path = downloader
        .get_file_path()
        .to_str()
        .unwrap_or_default()
        .to_string();

    let id = general_purpose::STANDARD.encode(file_path.as_bytes());

    // 启动下载任务并立即返回
    tokio::spawn({
        let id = id.clone();
        let downloader = Arc::new(Mutex::new(downloader));
        async move {
            info!("Prepare download，准备下载");
            let download_future = downloader.lock().await.prepare_download().unwrap();

            // 打印下载进度
            tokio::spawn({
                let mut downloaded_len_receiver =
                    downloader.lock().await.downloaded_len_receiver().clone();

                let total_size_future = downloader.lock().await.total_size_future();

                async move {
                    let total_len = total_size_future.await;
                    if let Some(total_len) = total_len {
                        info!(
                            "Total size: {:.2} Mb",
                            total_len.get() as f64 / 1024_f64 / 1024_f64
                        );
                    }
                    while downloaded_len_receiver.changed().await.is_ok() {
                        let progress = *downloaded_len_receiver.borrow();
                        if let Some(total_len) = total_len {
                            info!(
                                "Download Progress: {} %，{}/{}",
                                progress * 100 / total_len,
                                progress,
                                total_len
                            );

                            let full_path = downloader.lock().await.get_file_path();

                            let file_name =
                                full_path.file_name().unwrap().to_str().unwrap().to_string();

                            let d = downloader.lock().await;
                            let config = d.config();
                            let url_text = config.url.to_string();

                            let wrapper = NalaiWrapper {
                                downloader: downloader.clone(),
                                info: NalaiDownloadInfo {
                                    downloaded_bytes: progress,
                                    total_size: total_len,
                                    file_name: file_name,
                                    url: url_text,
                                    status: format!("{:?}",status_state.status())
                                },
                            };

                            GLOBAL_WRAPPERS.lock().await.insert(id.clone(), wrapper);
                        }
                        tokio::time::sleep(Duration::from_millis(1000)).await;
                    }
                }
            });

            info!("Start downloading until the end，开始下载直到结束");
            let dec = download_future.await.unwrap();
            info!("Downloading end cause: {:?}", dec);
        }
    });

    let result = json!({"id": &id});
    res.render(result.to_string());
}

fn convet_status(status: DownloaderStatus) -> StatusWrapper {
    match status {
        DownloaderStatus::NoStart => StatusWrapper::NoStart,
        DownloaderStatus::Running => StatusWrapper::Running,
        DownloaderStatus::Pending(i) => StatusWrapper::Pending(format!("{:?}", i)),
        DownloaderStatus::Error(e) => StatusWrapper::Error(e.to_string()),
        DownloaderStatus::Finished => StatusWrapper::Finished,
    }
}

#[handler]
async fn get_status(req: &mut Request, res: &mut Response) {
    let id = req.query::<String>("id");

    match id {
        Some(id) => {
            info!("Get status for id: {}", id);

            let wrapper = GLOBAL_WRAPPERS.lock().await.get(&id).cloned();

            if wrapper.is_none() {
                res.render(json!({"error": "id not found"}).to_string());
                return;
            }

            let status = wrapper.unwrap().info.clone();

            res.render(Json(status))
        }
        None => res.render(json!({"error": "id is required"}).to_string()),
    }
}

#[handler]
async fn stop_download(req: &mut Request, res: &mut Response) {
    let id = req.query::<String>("id").unwrap_or_default();
    let downloader = match GLOBAL_WRAPPERS.lock().await.get(&id) {
        Some(dl) => dl.downloader.clone(),
        None => {
            res.render(json!({"error": "id not found"}).to_string());
            return;
        }
    };
    info!("Stop download for id: {}", id);
    downloader.lock().await.cancel().await;
    res.render(json!({"success": true}).to_string());
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    info!("Starting server");

    tokio::spawn(async {
        let router = Router::new()
            .push(Router::with_path("/download").post(start_download))
            .push(Router::with_path("/status").get(get_status))
            .push(Router::with_path("/stop").post(stop_download));
        let acceptor = TcpListener::new("127.0.0.1:13088").bind().await;
        Server::new(acceptor).serve(router).await;
    });

    loop {
        thread::sleep(Duration::from_secs(1));
    }
}
