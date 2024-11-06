use once_cell::sync::Lazy;
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::num::{NonZeroU8, NonZeroUsize};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::Mutex;

use anyhow::{Ok, Result};
use tracing::info;
use url::Url;

use http_downloader::bson_file_archiver::{ArchiveFilePath, BsonFileArchiverBuilder};
use http_downloader::speed_limiter::DownloadSpeedLimiterExtension;
use http_downloader::{
    breakpoint_resume::DownloadBreakpointResumeExtension,
    speed_tracker::DownloadSpeedTrackerExtension, status_tracker::DownloadStatusTrackerExtension,
    HttpDownloaderBuilder,
};

static DOWNLOADER_CREATOR: Lazy<fn() -> HttpDownloaderBuilder> = Lazy::new(|| create_downloader);
static GLOBAL_DOWNLOAD_TASKS: Lazy<Mutex<Vec<Status>>> = Lazy::new(|| Mutex::new(Vec::new()));

fn create_downloader() -> HttpDownloaderBuilder {
    let save_dir = PathBuf::from("C:/download");
    let test_url =
        Url::parse("https://mirrors.tuna.tsinghua.edu.cn/debian-cd/current/amd64/iso-cd/debian-12.7.0-amd64-netinst.iso")
            .unwrap();
    HttpDownloaderBuilder::new(test_url, save_dir)
        .chunk_size(NonZeroUsize::new(1024 * 1024 * 10).unwrap()) // 块大小
        .download_connection_count(NonZeroU8::new(8).unwrap())
        .file_name(Some("file_name.zip".to_string()))
}

#[handler]
async fn start_download() -> String {
    let (mut downloader, (status_state, speed_state, speed_limiter, ..)) = (DOWNLOADER_CREATOR)()
        .build((
            DownloadStatusTrackerExtension { log: true },
            DownloadSpeedTrackerExtension { log: true },
            DownloadSpeedLimiterExtension::new(None),
            DownloadBreakpointResumeExtension {
                download_archiver_builder: BsonFileArchiverBuilder::new(ArchiveFilePath::Suffix(
                    "bson".to_string(),
                )),
            },
        ));

    let download_future = downloader.prepare_download().unwrap();

    // let _status = status_state.status(); // get download status， 获取状态
    // let _status_receiver = status_state.status_receiver; //status watcher，状态监听器
    // let _byte_per_second = speed_state.download_speed(); // get download speed，Byte per second，获取速度，字节每秒
    let _speed_receiver = speed_state.receiver; // get download speed watcher，速度监听器

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
                }

                tokio::time::sleep(Duration::from_millis(1000)).await;
            }
        }
    });

    // 下载速度限制
    // Download speed limit
    // tokio::spawn(async move {
    //     tokio::time::sleep(Duration::from_secs(2)).await;
    //     info!("Start speed limit，开始限速");
    //     speed_limiter.change_speed(Some(1024 * 1024 * 2)).await;
    //     // downloader.cancel().await; // 取消下载
    //     tokio::time::sleep(Duration::from_secs(4)).await;
    //     info!("Remove the download speed limit，解除速度限制");
    //     speed_limiter.change_speed(None).await;
    // });

    info!("Start downloading until the end，开始下载直到结束");

    let result = format!("Start downloading{:?}", download_future.await.unwrap());
    result
}

#[derive(Serialize, Deserialize)]
struct Status {
    id: String,
    status: String,
    progress: u64,
    speed: u64,
}

#[handler]
async fn get_status(req: &mut Request, res: &mut Response) {
    let id = req.query::<String>("id").unwrap_or_default();

    info!("Get status for id: {}", id);

    let status = Status {
        id: id.clone(),
        status: "Downloading".to_string(),
        progress: 0,
        speed: 0,
    };

    res.render(Json(status))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    info!("Starting server");

    tokio::spawn(async {
        let router = Router::new()
            .push(Router::with_path("/download").get(start_download))
            .push(Router::with_path("/status").get(get_status));
        let acceptor = TcpListener::new("127.0.0.1:13088").bind().await;
        Server::new(acceptor).serve(router).await;
    });

    loop {}

    // let dec = download_future.await?;
    // info!("Downloading end cause: {:?}", dec);
}
