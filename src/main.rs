use base64::{engine::general_purpose, Engine};
use http_downloader::{
    breakpoint_resume::DownloadBreakpointResumeExtension,
    bson_file_archiver::{ArchiveFilePath, BsonFileArchiverBuilder},
    speed_limiter::DownloadSpeedLimiterExtension,
    speed_tracker::DownloadSpeedTrackerExtension,
    status_tracker::{DownloadStatusTrackerExtension, DownloaderStatus},
    DownloadingEndCause, ExtendedHttpFileDownloader, HttpDownloaderBuilder,
};
use once_cell::sync::Lazy;
use salvo::{http::form, prelude::*};
use serde::{Deserialize, Serialize};
use serde_json::{json, to_value, Value};
use std::{
    collections::HashMap,
    num::{NonZero, NonZeroU8, NonZeroUsize},
    path::PathBuf,
    result,
    sync::Arc,
    thread,
    time::Duration,
};
use tokio::sync::Mutex;
use tracing::info;
use url::Url;

static GLOBAL_WRAPPERS: Lazy<Arc<Mutex<HashMap<String, NalaiWrapper>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

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
    speed: u64,
}

impl Default for NalaiDownloadInfo {
    fn default() -> Self {
        Self {
            downloaded_bytes: Default::default(),
            total_size: NonZero::new(1).unwrap(),
            file_name: Default::default(),
            url: Default::default(),
            status: Default::default(),
            speed: Default::default(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
enum StatusWrapper {
    NoStart,
    Running,
    Pending(String),
    Error(String),
    Finished,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct NalaiResult {
    success: bool,
    code: String,
    data: Value,
}

impl NalaiResult {
    fn new(success: bool, code: StatusCode, data: Value) -> Self {
        Self {
            success,
            code: code.to_string(),
            data,
        }
    }
}

#[handler]
async fn start_download(req: &mut Request, res: &mut Response) {
    let save_dir = req.query::<String>("save_dir").unwrap_or_default();
    let save_dir = PathBuf::from(save_dir);
    let url = req.query::<String>("url").unwrap_or_default();
    let url = Url::parse(&url).unwrap();

    let (mut downloader, (mut status_state, mut speed_state, speed_limiter, ..)) =
        HttpDownloaderBuilder::new(url, save_dir)
            .chunk_size(NonZeroUsize::new(1024 * 1024 * 10).unwrap()) // 块大小
            .download_connection_count(NonZeroU8::new(3).unwrap())
            .downloaded_len_send_interval(Some(Duration::from_millis(100)))
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
    let id2 = id.clone();
    let id3 = id.clone();

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

                let state_receiver = downloader.lock().await.downloading_state_receiver();

                // TODO: 虽说先初始化一次是没什么毛病，但总感觉不够优雅
                let wrapper = NalaiWrapper {
                    downloader: downloader.clone(),
                    info: NalaiDownloadInfo::default(),
                };

                GLOBAL_WRAPPERS.lock().await.insert(id.clone(), wrapper);

                async move {
                    let total_len = total_size_future.await;
                    if let Some(total_len) = total_len {
                        info!(
                            "Total size: {:.2} Mb",
                            total_len.get() as f64 / 1024_f64 / 1024_f64
                        );
                    }
                    // 实则是接收下载进度的说
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
                                    status: format!("{:?}", &status_state.status()),
                                    speed: speed_state.download_speed(),
                                },
                            };

                            GLOBAL_WRAPPERS.lock().await.insert(id.clone(), wrapper);
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    // 实则是接收状态更新的说
                    while status_state.status_receiver.changed().await.is_ok() {
                        println!("State update: {:?}", status_state.status());
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
                                    status: format!("{:?}", status_state.status()),
                                    speed: speed_state.download_speed(),
                                },
                            };

                            GLOBAL_WRAPPERS.lock().await.insert(id.clone(), wrapper);

                            if let DownloaderStatus::Error(e) = status_state.status() {
                                info!("Download error: {}", e);
                                break;
                            }
                            if let DownloaderStatus::Finished = status_state.status() {
                                info!("Download finished");
                                break;
                            }
                        }

                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    // 实则是接收下载速度的说
                    while speed_state.receiver.changed().await.is_ok() {
                        let speed = speed_state.download_speed();
                        GLOBAL_WRAPPERS
                            .lock()
                            .await
                            .get_mut(&id2.clone())
                            .unwrap()
                            .info
                            .speed = speed;
                        info!("Download speed: {} bytes/s", speed)
                    }
                }
            });

            info!("Start downloading until the end，开始下载直到结束");
            let dec = download_future.await;

            let result = match dec {
                Ok(msg) => format!("{:?}", msg),
                Err(msg) => format!("{:?}", msg),
            };

            {
                let mut lock = GLOBAL_WRAPPERS.lock().await;
                if let Some(wrapper) = lock.get_mut(&id3.clone()) {
                    wrapper.info.status = result.clone();
                }
            }

            info!("Downloading end cause: {}", result);
        }
    });

    let result = NalaiResult::new(true, StatusCode::OK, json!({"id": &id}));
    res.render(Json(result));
}

#[handler]
async fn cancel_or_start_download_api(req: &mut Request, res: &mut Response) {
    let id = req.query::<String>("id").unwrap_or_default();
    let result = match cancel_or_start_download(&id).await {
        Ok(success) => {
            if success {
                NalaiResult::new(true, StatusCode::OK, Value::Null)
            } else {
                NalaiResult::new(false, StatusCode::BAD_REQUEST, Value::Null)
            }
        }
        Err(e) => NalaiResult::new(
            false,
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"error": e}),
        ),
    };
    res.render(Json(result));
}

async fn cancel_or_start_download(id: &str) -> Result<bool, String> {
    let lock = GLOBAL_WRAPPERS.lock().await;
    let wrapper = match lock.get(id) {
        Some(dl) => dl,
        None => return Err(format!("No such download id: {}", id)),
    };

    let status = wrapper.info.status.clone();
    let no_start = format!("{:?}", DownloaderStatus::NoStart);
    let running = format!("{:?}", DownloaderStatus::Running);
    let finished = format!("{:?}", DownloaderStatus::Finished);
    let canceled = format!("{:?}", DownloadingEndCause::Cancelled);
    let download_finished = format!("{:?}", DownloadingEndCause::DownloadFinished);

    println!("Status: {:?}", &status);

    if status == no_start || status == canceled {
        // 未开始下载，直接开始下载
        let downloader = wrapper.downloader.clone();
        start_download();
        Ok(true)
    } else if status == running {
        // 正在下载，取消下载
        let downloader = wrapper.downloader.clone();
        downloader.lock().await.cancel().await;
        Ok(true)
    } else if status == finished {
        // 下载完成，不做任何操作
        Ok(false)
    } else if status == download_finished {
        // 下载完成，不做任何操作
        Ok(false)
    } else {
        // 其他状态，不做任何操作
        Ok(false)
    }
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
async fn get_info(req: &mut Request, res: &mut Response) {
    let id = req.query::<String>("id");

    match id {
        Some(id) => {
            info!("Get status for id: {}", id);

            let wrapper = {
                let lock = GLOBAL_WRAPPERS.lock().await;
                lock.get(&id).cloned()
            };

            if wrapper.is_none() {
                let result = NalaiResult::new(false, StatusCode::NOT_FOUND, Value::Null);
                res.render(Json(result));
                return;
            }

            let info = wrapper.unwrap().info.clone();

            let result = NalaiResult::new(true, StatusCode::OK, to_value(info).unwrap());

            res.render(Json(result));
        }
        None => {
            let result = NalaiResult::new(false, StatusCode::BAD_REQUEST, Value::Null);

            res.render(Json(result));
        }
    }
}

#[handler]
async fn stop_download(req: &mut Request, res: &mut Response) {
    let id = req.query::<String>("id").unwrap_or_default();
    let downloader = match GLOBAL_WRAPPERS.lock().await.get(&id) {
        Some(dl) => dl.downloader.clone(),
        None => {
            let result = NalaiResult::new(false, StatusCode::NOT_FOUND, Value::Null);
            res.render(Json(result));
            return;
        }
    };
    info!("Stop download for id: {}", id);
    downloader.lock().await.cancel().await;

    let result = NalaiResult::new(true, StatusCode::OK, Value::Null);
    res.render(Json(result));
}

#[handler]
async fn get_all_info_api(_req: &mut Request, res: &mut Response) {
    let all_info = get_all_info().await;
    let result = NalaiResult::new(true, StatusCode::OK, to_value(all_info).unwrap());
    res.render(Json(result));
}

async fn get_all_info() -> HashMap<String, NalaiDownloadInfo> {
    let lock = GLOBAL_WRAPPERS.lock().await;
    let mut result = HashMap::new();
    for (id, wrapper) in lock.iter() {
        result.insert(id.clone(), wrapper.info.clone());
    }
    result
}

async fn save_to_file() -> Result<(), std::io::Error> {
    let all_info = get_all_info().await;
    let json_str = serde_json::to_string(&all_info).unwrap();
    let file_path = PathBuf::from("nalai_info_data.json");
    std::fs::write(file_path, json_str).unwrap();
    Ok(())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    info!("Starting server");

    tokio::spawn(async {
        let router = Router::new()
            .push(Router::with_path("/download").post(start_download))
            .push(Router::with_path("/info").get(get_info))
            .push(Router::with_path("/stop").post(stop_download))
            .push(Router::with_path("/all_info").get(get_all_info_api))
            .push(Router::with_path("/sorc").post(cancel_or_start_download_api));
        let acceptor = TcpListener::new("127.0.0.1:13088").bind().await;
        Server::new(acceptor).serve(router).await;
    });

    // ctrlc::set_handler(async || {
    //     println!("收到中断信号，正在保存数据...");
    //     if let Err(e) = save_to_file().await {
    //         eprintln!("保存数据时出错: {}", e);
    //     } else {
    //         println!("数据已成功保存");
    //     }
    //     std::process::exit(0);
    // }).unwrap();

    loop {
        thread::sleep(Duration::from_secs(1));
    }
}
