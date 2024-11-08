use base64::engine::general_purpose;
use base64::Engine;
use http_downloader::{DownloadingEndCause, ExtendedHttpFileDownloader};
use once_cell::sync::Lazy;
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use std::borrow::Borrow;
use std::collections::HashMap;
use std::num::{NonZeroU8, NonZeroUsize};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio::sync::Mutex;

use tracing::info;
use url::Url;

use http_downloader::bson_file_archiver::{ArchiveFilePath, BsonFileArchiverBuilder};
use http_downloader::speed_limiter::DownloadSpeedLimiterExtension;
use http_downloader::{
    breakpoint_resume::DownloadBreakpointResumeExtension,
    speed_tracker::DownloadSpeedTrackerExtension, status_tracker::DownloadStatusTrackerExtension,
    HttpDownloaderBuilder,
};

mod play_ground;

static DOWNLOADER_CREATOR: Lazy<
    fn(url: Url, save_dir: PathBuf, file_name: Option<String>) -> HttpDownloaderBuilder,
> = Lazy::new(|| create_downloader);

static GLOBAL_STATUS_LIST: Lazy<Arc<Mutex<HashMap<String, NalaiInfo>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

static GLOBAL_DOWNLOADERS: Lazy<Mutex<HashMap<String, Arc<Mutex<NalaiDownloaderWrapper>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

struct NalaiDownloaderWrapper {
    id: String,
    downloader: Arc<Mutex<ExtendedHttpFileDownloader>>,
}

impl NalaiDownloaderWrapper {
    pub fn new(id: String, downloader: Arc<Mutex<ExtendedHttpFileDownloader>>) -> Self {
        Self { id, downloader }
    }

    pub async fn start_download(&self) -> DownloadingEndCause {
        let result = self.downloader.lock().await.prepare_download().unwrap();
        // self.print_progress().await;
        result.await.unwrap()
    }

    pub async fn print_progress(&self) {
        let downloader = self.downloader.clone();
        tokio::spawn({
            let id = self.id.clone();
            let downloaded_len_receiver = downloader.lock().await;
            let mut downloaded_len_receiver = downloaded_len_receiver
                .borrow()
                .downloaded_len_receiver()
                .clone();
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
                    }

                    let mut info = GLOBAL_STATUS_LIST
                        .lock()
                        .await
                        .get(&id)
                        .unwrap_or(&NalaiInfo::default())
                        .clone();

                    info.progress = progress * 100 / total_len.unwrap();

                    GLOBAL_STATUS_LIST
                        .lock()
                        .await
                        .insert(id.clone(), info.clone());

                    tokio::time::sleep(Duration::from_millis(1000)).await;
                }
            }
        });
    }
}

fn create_downloader(
    url: Url,
    save_dir: PathBuf,
    file_name: Option<String>,
) -> HttpDownloaderBuilder {
    HttpDownloaderBuilder::new(url, save_dir)
        .chunk_size(NonZeroUsize::new(1024 * 1024 * 10).unwrap()) // 块大小
        .download_connection_count(NonZeroU8::new(8).unwrap())
        .file_name(file_name)
    // .file_name(Some("file_name.zip".to_string()))
}

#[handler]
async fn start_download(req: &mut Request, res: &mut Response) -> String {
    let file_name: Option<String> = Some(req.query::<String>("file_name")).unwrap_or(None);
    let save_dir = req.query::<String>("save_dir").unwrap_or_default();
    let save_dir = PathBuf::from(save_dir);
    let url = req.query::<String>("url").unwrap_or_default();
    let url = Url::parse(&url).unwrap();

    let (mut downloader, (status_state, speed_state, speed_limiter, ..)) =
        (DOWNLOADER_CREATOR)(url, save_dir, file_name).build((
            DownloadStatusTrackerExtension { log: true },
            DownloadSpeedTrackerExtension { log: true },
            DownloadSpeedLimiterExtension::new(None),
            DownloadBreakpointResumeExtension {
                download_archiver_builder: BsonFileArchiverBuilder::new(ArchiveFilePath::Suffix(
                    "bson".to_string(),
                )),
            },
        ));

    let file_path = downloader
        .get_file_path()
        .to_str()
        .unwrap_or_default()
        .to_string();

    let id = general_purpose::STANDARD.encode(file_path.as_bytes());

    let downloader = Arc::new(Mutex::new(downloader));

    GLOBAL_DOWNLOADERS.lock().await.insert(
        id.clone(),
        Arc::new(Mutex::new(NalaiDownloaderWrapper::new(
            id.clone(),
            downloader,
        ))),
    );

    GLOBAL_DOWNLOADERS
        .lock()
        .await
        .get(&id)
        .unwrap()
        .lock()
        .await
        .start_download()
        .await;

    "Started".to_string()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct NalaiInfo {
    status: String,
    progress: u64,
    speed: u64,
}

impl Default for NalaiInfo {
    fn default() -> Self {
        Self {
            status: Default::default(),
            progress: Default::default(),
            speed: Default::default(),
        }
    }
}

#[handler]
async fn get_info(req: &mut Request, res: &mut Response) {
    let id = req.query::<String>("id").unwrap_or_default();

    info!("Get status for id: {}", id);

    let status = GLOBAL_STATUS_LIST.lock().await.get(&id).cloned().unwrap();

    res.render(Json(status))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    info!("Starting server");

    tokio::spawn(async {
        let router = Router::new()
            .push(Router::with_path("/download").post(start_download))
            .push(Router::with_path("/info").get(get_info));
        let acceptor = TcpListener::new("127.0.0.1:13088").bind().await;
        Server::new(acceptor).serve(router).await;
    });

    // play_ground::main();

    loop {
        thread::sleep(Duration::from_secs(10));
    }

    // let dec = download_future.await?;
    // info!("Downloading end cause: {:?}", dec);
}
