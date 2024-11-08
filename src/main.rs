use base64::{engine::general_purpose, Engine};
use http_downloader::{
    breakpoint_resume::DownloadBreakpointResumeExtension,
    bson_file_archiver::{ArchiveFilePath, BsonFileArchiverBuilder},
    speed_limiter::DownloadSpeedLimiterExtension,
    speed_tracker::DownloadSpeedTrackerExtension,
    status_tracker::DownloadStatusTrackerExtension,
    HttpDownloaderBuilder,
};
use once_cell::sync::Lazy;
use salvo::prelude::*;
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

static GLOBAL_DOWNLOADERS: Lazy<Mutex<HashMap<String, NalaiStatus>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
struct NalaiStatus {
    downloaded_bytes: u64,
    total_size: NonZero<u64>,
    file_name: String,
    url: String,
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
        async move {
            info!("Prepare download，准备下载");
            let download_future = downloader.prepare_download().unwrap();

            // 打印下载进度
            tokio::spawn({
                let mut downloaded_len_receiver = downloader.downloaded_len_receiver().clone();

                let total_size_future = downloader.total_size_future();

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

                            let full_path = downloader.get_file_path();

                            let file_name =
                                full_path.file_name().unwrap().to_str().unwrap().to_string();

                            let config = downloader.config();
                            let url_text = config.url.to_string();

                            GLOBAL_DOWNLOADERS.lock().await.insert(
                                id.clone(),
                                NalaiStatus {
                                    downloaded_bytes: progress,
                                    total_size: total_len,
                                    file_name: file_name,
                                    url: url_text,
                                },
                            );
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

#[handler]
async fn get_status(req: &mut Request, res: &mut Response) {
    let id = req.query::<String>("id");

    match id {
        Some(id) => {
            info!("Get status for id: {}", id);

            let status = GLOBAL_DOWNLOADERS.lock().await.get(&id).cloned();

            if status.is_none() {
                res.render(json!({"error": "id not found"}).to_string());
                return;
            }

            res.render(Json(status))
        }
        None => res.render(json!({"error": "id is required"}).to_string()),
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    info!("Starting server");

    tokio::spawn(async {
        let router = Router::new()
            .push(Router::with_path("/download").post(start_download))
            .push(Router::with_path("/status").get(get_status));
        let acceptor = TcpListener::new("127.0.0.1:13088").bind().await;
        Server::new(acceptor).serve(router).await;
    });

    loop {
        thread::sleep(Duration::from_secs(1));
    }
}
