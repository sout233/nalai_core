use base64::{engine::general_purpose, Engine};
use http_downloader::{
    breakpoint_resume::DownloadBreakpointResumeExtension,
    bson_file_archiver::{ArchiveFilePath, BsonFileArchiverBuilder},
    speed_limiter::DownloadSpeedLimiterExtension,
    speed_tracker::DownloadSpeedTrackerExtension,
    status_tracker::{DownloadStatusTrackerExtension, DownloaderStatus},
    DownloadError, DownloadingEndCause, ExtendedHttpFileDownloader, HttpDownloaderBuilder,
};
use once_cell::sync::Lazy;
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, to_value, Value};
use std::{
    collections::HashMap,
    fmt::Display,
    num::{NonZero, NonZeroU8, NonZeroUsize},
    path::PathBuf,
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
    downloader: Option<Arc<Mutex<ExtendedHttpFileDownloader>>>,
    info: NalaiDownloadInfo,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct NalaiDownloadInfo {
    downloaded_bytes: u64,
    total_size: NonZero<u64>,
    file_name: String,
    url: String,
    status: StatusWrapper,
    speed: u64,
    save_dir: String,
}

impl Default for NalaiDownloadInfo {
    fn default() -> Self {
        Self {
            downloaded_bytes: Default::default(),
            total_size: NonZero::new(1).unwrap(),
            file_name: Default::default(),
            url: Default::default(),
            status: StatusWrapper::NoStart,
            speed: Default::default(),
            save_dir: Default::default(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
enum StatusWrapper {
    NoStart,
    Running,
    Pending,
    Error,
    Finished,
}

impl Display for StatusWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StatusWrapper::NoStart => write!(f, "NoStart"),
            StatusWrapper::Running => write!(f, "Running"),
            StatusWrapper::Pending => write!(f, "Pending"),
            StatusWrapper::Error => write!(f, "Error"),
            StatusWrapper::Finished => write!(f, "Finished"),
        }
    }
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

async fn insert_to_global_wrappers(id: String, wrapper: NalaiWrapper) {
    let mut global_wrappers = GLOBAL_WRAPPERS.lock().await;
    global_wrappers.insert(id, wrapper);

    // let _ = save_all_to_file().await;
}

#[handler]
async fn start_download_api(req: &mut Request, res: &mut Response) {
    let save_dir = req.query::<String>("save_dir").unwrap_or_default();
    let save_dir = PathBuf::from(save_dir);
    let url = req.query::<String>("url").unwrap_or_default();
    let url = Url::parse(&url).unwrap();

    let id = start_download(&url, &save_dir);

    let result = NalaiResult::new(true, StatusCode::OK, json!({"id": &id}));
    res.render(Json(result));
}

fn start_download(url: &Url, save_dir: &PathBuf) -> String {
    let (downloader, (mut status_state, mut speed_state, _speed_limiter, ..)) =
        HttpDownloaderBuilder::new(url.clone(), save_dir.clone())
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

    info!("spawn download task，启动下载任务");

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

                let _state_receiver = downloader.lock().await.downloading_state_receiver();

                // TODO: 虽说先初始化一次是没什么毛病，但总感觉不够优雅
                let wrapper = NalaiWrapper {
                    downloader: Some(downloader.clone()),
                    info: NalaiDownloadInfo::default(),
                };

                insert_to_global_wrappers(id.clone(), wrapper).await;

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
                                downloader: Some(downloader.clone()),
                                info: NalaiDownloadInfo {
                                    downloaded_bytes: progress,
                                    total_size: total_len,
                                    file_name: file_name,
                                    url: url_text,
                                    status: convert_status(DownloaderStatusWrapper::from(
                                        status_state.status(),
                                    )),
                                    speed: speed_state.download_speed(),
                                    save_dir: config.save_dir.to_str().unwrap().to_string(),
                                },
                            };

                            insert_to_global_wrappers(id.clone(), wrapper).await;
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
                                downloader: Some(downloader.clone()),
                                info: NalaiDownloadInfo {
                                    downloaded_bytes: progress,
                                    total_size: total_len,
                                    file_name: file_name,
                                    url: url_text,
                                    status: convert_status(DownloaderStatusWrapper::from(
                                        status_state.status(),
                                    )),
                                    speed: speed_state.download_speed(),
                                    save_dir: config.save_dir.to_str().unwrap().to_string(),
                                },
                            };

                            insert_to_global_wrappers(id.clone(), wrapper).await;

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
                Ok(msg) => convert_status(msg),
                Err(msg) => convert_status(msg),
            };

            {
                let mut lock = GLOBAL_WRAPPERS.lock().await;
                if let Some(wrapper) = lock.get_mut(&id3.clone()) {
                    wrapper.info.status = result.clone();
                }
            }

            info!("Downloading end cause: {:?}", result);
        }
    });

    info!("Download task started，下载任务已启动");

    // save_all_to_file().await.unwrap();

    id
}

#[handler]
async fn cancel_or_start_download_api(req: &mut Request, res: &mut Response) {
    let id = req.query::<String>("id").unwrap_or_default();
    let result = match cancel_or_start_download(&id).await {
        Ok((success, running)) => {
            if success {
                NalaiResult::new(true, StatusCode::OK, json!({"running": running}))
            } else {
                NalaiResult::new(
                    false,
                    StatusCode::BAD_REQUEST,
                    json!({"error": "Task is Finished or Error"}),
                )
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

async fn cancel_or_start_download(id: &str) -> Result<(bool, bool), String> {
    let info = match get_info(id).await {
        Some(it) => it,
        None => return Err(format!("No such download id: {}", id)),
    };

    let status = info.status.clone();

    let wrapper = match get_wrapper_by_id(id).await {
        Some(it) => it,
        None => return Err(format!("No such download id: {}", id)),
    };

    match status {
        StatusWrapper::NoStart => {
            // 未开始下载，直接开始下载
            let url = Url::parse(&wrapper.info.url).unwrap();
            let save_dir = PathBuf::from(&wrapper.info.save_dir);
            start_download(&url, &save_dir);
            Ok((true, true))
        }
        StatusWrapper::Running => {
            // 正在下载，取消下载
            cancel_download(id).await?;
            Ok((true, false))
        }
        StatusWrapper::Pending => {
            // 等待下载，取消下载
            cancel_download(id).await?;
            Ok((true, false))
        }
        StatusWrapper::Error => {
            // 下载出错，重新开始下载
            let url = Url::parse(&wrapper.info.url).unwrap();
            let save_dir = PathBuf::from(&wrapper.info.save_dir);
            start_download(&url, &save_dir);
            Ok((true, true))
        }
        StatusWrapper::Finished => {
            // 下载完成，不做任何操作
            Ok((false, false))
        }
    }
}

async fn cancel_download(id: &str) -> anyhow::Result<bool, String> {
    let lock = GLOBAL_WRAPPERS.lock().await;
    let wrapper = match lock.get(id) {
        Some(dl) => dl,
        None => return Err(format!("No such download id: {}", id)),
    };

    let downloader = wrapper.downloader.clone().unwrap();
    downloader.lock().await.cancel().await;

    match save_all_to_file().await {
        Ok(it) => it,
        Err(err) => return Err(err.to_string()),
    };

    Ok(true)
}

trait IntoStatusWrapper {
    fn into_status_wrapper(self) -> StatusWrapper;
}

// 创建一个新的包装类型
struct DownloaderStatusWrapper(DownloaderStatus);

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

fn convert_status<T>(status: T) -> StatusWrapper
where
    T: IntoStatusWrapper,
{
    status.into_status_wrapper()
}

#[handler]
async fn get_info_api(req: &mut Request, res: &mut Response) {
    let id = req.query::<String>("id");

    match id {
        Some(id) => {
            info!("Get status for id: {}", id);

            let info = match get_info(&id).await {
                Some(info) => info,
                None => {
                    let result = NalaiResult::new(false, StatusCode::NOT_FOUND, Value::Null);
                    res.render(Json(result));
                    return;
                }
            };

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
async fn exit_api(_req: &mut Request, res: &mut Response) {
    info!("收到中断信号，正在保存数据...");
    if let Err(e) = save_all_to_file().await {
        eprintln!("保存数据时出错: {}", e);
        let result = NalaiResult::new(
            false,
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"error":e.to_string()}),
        );
        res.render(Json(result));
    } else {
        info!("数据已成功保存");
        let result = NalaiResult::new(true, StatusCode::OK, Value::Null);
        res.render(Json(result));
        std::process::exit(0);
    }
}

async fn get_info(id: &str) -> Option<NalaiDownloadInfo> {
    let wrapper = get_wrapper_by_id(id).await;

    if wrapper.is_none() {
        return None;
    }

    let info = wrapper.unwrap().info.clone();
    Some(info)
}

async fn get_wrapper_by_id(id: &str) -> Option<NalaiWrapper> {
    let lock = GLOBAL_WRAPPERS.lock().await;
    lock.get(id).cloned()
}

#[handler]
async fn cancel_download_api(req: &mut Request, res: &mut Response) {
    let id = req.query::<String>("id").unwrap_or_default();

    match cancel_download(&id).await {
        Ok(success) => {
            if success {
                let result = NalaiResult::new(true, StatusCode::OK, Value::Null);
                res.render(Json(result));
            } else {
                let result = NalaiResult::new(false, StatusCode::NOT_FOUND, Value::Null);
                res.render(Json(result));
            }
        }
        Err(e) => {
            let result = NalaiResult::new(
                false,
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error": e}),
            );
            res.render(Json(result));
        }
    }
}

#[handler]
async fn get_all_info_api(_req: &mut Request, res: &mut Response) {
    save_all_to_file().await.unwrap();
    let all_info = get_all_info().await;
    let result = NalaiResult::new(true, StatusCode::OK, to_value(all_info).unwrap());
    res.render(Json(result));
}

#[handler]
async fn delete_download_api(req: &mut Request, res: &mut Response) {
    let id = req.query::<String>("id").unwrap_or_default();

    match delete_download(&id).await {
        Ok(success) => {
            let all_info = get_all_info().await;
            if success {
                let result = NalaiResult::new(true, StatusCode::OK, to_value(all_info).unwrap());
                res.render(Json(result));
            } else {
                let result =
                    NalaiResult::new(false, StatusCode::NOT_FOUND, to_value(all_info).unwrap());
                res.render(Json(result));
            }
        }
        Err(e) => {
            let result = NalaiResult::new(
                false,
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error": e}),
            );
            res.render(Json(result));
        }
    }
}

#[handler]
async fn check_health_api(_req: &mut Request, res: &mut Response) {
    let result = NalaiResult::new(true, StatusCode::OK, Value::Null);
    res.render(Json(result));
}

async fn delete_download(id: &str) -> anyhow::Result<bool, String> {
    cancel_download(id).await?;

    let value = GLOBAL_WRAPPERS.lock().await.remove(id);

    let r = match value {
        Some(_) => {
            let a: Result<bool, String> = match save_all_to_file().await {
                Ok(_) => Ok(true),
                Err(err) => return Err(err.to_string()),
            };
            a
        }
        None => Ok(false),
    };

    r
}

async fn get_all_info() -> HashMap<String, NalaiDownloadInfo> {
    let lock = GLOBAL_WRAPPERS.lock().await;
    let mut result = HashMap::new();
    for (id, wrapper) in lock.iter() {
        result.insert(id.clone(), wrapper.info.clone());
    }
    result
}

async fn load_global_wrappers_from_json() {
    let file_path = PathBuf::from("nalai_info_data.json");
    if file_path.exists() {
        let json_str = std::fs::read_to_string(file_path).unwrap();
        let all_info: HashMap<String, NalaiDownloadInfo> = serde_json::from_str(&json_str).unwrap();
        let mut lock = GLOBAL_WRAPPERS.lock().await;
        for (id, info) in all_info.iter() {
            let wrapper = NalaiWrapper {
                downloader: None,
                info: info.clone(),
            };
            lock.insert(id.clone(), wrapper);
        }
    }
}

async fn save_all_to_file() -> anyhow::Result<()> {
    tokio::spawn(async {
        let all_info = get_all_info().await;
        let json_str = serde_json::to_string(&all_info).unwrap();
        let file_path = PathBuf::from("nalai_info_data.json");
        tokio::fs::write(file_path.clone(), json_str).await.unwrap();
        info!("数据已保存到文件: {}", file_path.to_str().unwrap());
    });
    Ok(())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    info!("Starting server");

    load_global_wrappers_from_json().await;

    tokio::spawn(async {
        let router = Router::new()
            .push(
                Router::with_path("/download")
                    .post(start_download_api)
                    .delete(delete_download_api),
            )
            .push(Router::with_path("/info").get(get_info_api))
            .push(Router::with_path("/cancel").post(cancel_download_api))
            .push(Router::with_path("/all_info").get(get_all_info_api))
            .push(Router::with_path("/sorc").post(cancel_or_start_download_api))
            .push(Router::with_path("checkhealth").get(check_health_api))
            .push(Router::with_path("exit").get(exit_api));

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

    info!("Nalai Core 服务已启动 ヾ(≧▽≦*)o");

    loop {
        thread::sleep(Duration::from_secs(1));
    }
}
