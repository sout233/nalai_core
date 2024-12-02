use crate::{
    models::{
        chunk_wrapper::ChunkWrapper, nalai_download_info::NalaiDownloadInfo,
        nalai_result::NalaiResult, nalai_wrapper::NalaiWrapper, status_wrapper::StatusWrapper,
    },
    utils::{
        global_wrappers::{self, get_wrapper_by_id},
        status_conversion::{self, DownloaderStatusWrapper},
    },
};
use base64::{engine::general_purpose, Engine};
use http_downloader::{
    breakpoint_resume::DownloadBreakpointResumeExtension,
    bson_file_archiver::{ArchiveFilePath, BsonFileArchiverBuilder},
    speed_limiter::DownloadSpeedLimiterExtension,
    speed_tracker::DownloadSpeedTrackerExtension,
    status_tracker::{DownloadStatusTrackerExtension, DownloaderStatus},
    HttpDownloaderBuilder,
};
use salvo::prelude::*;
use serde_json::{json, to_value, Value};
use std::{
    num::{NonZeroU8, NonZeroUsize},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::sync::Mutex;
use tracing::info;
use url::Url;

use super::info;
#[handler]
pub async fn start_download_api(req: &mut Request, res: &mut Response) {
    let save_dir = req.query::<String>("save_dir").unwrap_or_default();
    let save_dir = PathBuf::from(save_dir);
    let url = req.query::<String>("url").unwrap_or_default();
    let url = Url::parse(&url).unwrap();
    let pre_id = req.query::<String>("id");
    let file_name = req.query::<String>("file_name");

    let id = start_download(&url, &save_dir, file_name, pre_id).await;

    let result = NalaiResult::new(true, StatusCode::OK, json!({"id": &id}));
    res.render(Json(result));
}

async fn start_download(
    url: &Url,
    save_dir: &PathBuf,
    file_name: Option<String>,
    mut id: Option<String>,
) -> String {
    let (downloader, (mut status_state, mut speed_state, _speed_limiter, ..)) =
        HttpDownloaderBuilder::new(url.clone(), save_dir.clone())
            .chunk_size(NonZeroUsize::new(1024 * 1024 * 10).unwrap()) // 块大小
            .download_connection_count(NonZeroU8::new(3).unwrap())
            .downloaded_len_send_interval(Some(Duration::from_millis(100)))
            .file_name(file_name)
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

    if id.is_none() {
        let file_path = downloader
            .get_file_path()
            .to_str()
            .unwrap_or_default()
            .to_string();

        id = Some(general_purpose::STANDARD.encode(file_path.as_bytes()));
    }

    let id = id.unwrap();
    let id2 = id.clone();
    let id3 = id.clone();

    let wrapper = get_wrapper_by_id(id.clone().as_str()).await;
    if !wrapper.is_none() {
        info!("wrapper is not none");
        let downloader = wrapper.clone().unwrap().downloader;
        if !downloader.is_none() {
            info!("downloader is not none");
            // let downloader = downloader.unwrap();
            // let mut status_state = downloader.lock().await.get_downloading_state().unwrap().upgrade().unwrap();
            let status = wrapper.as_ref().clone().unwrap().info.status.clone();
            info!("status is {}", status);
            if status == StatusWrapper::Running {
                info!("Download task already running，下载任务已运行");
                return id.clone();
            }
        }
    }

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
                // let wrapper = NalaiWrapper {
                //     downloader: Some(downloader.clone()),
                //     info: NalaiDownloadInfo::default(),
                // };

                // global_wrappers::insert_to_global_wrappers(id.clone(), wrapper).await;

                let mut original_info = NalaiDownloadInfo::default();
                let original_wrapper = global_wrappers::get_wrapper_by_id(&id.clone()).await;
                match original_wrapper {
                    Some(wrapper) => {
                        let info = wrapper.info.clone();
                        original_info = info;
                    }
                    None => {}
                }

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
                            let full_path = downloader.lock().await.get_file_path();

                            let file_name =
                                full_path.file_name().unwrap().to_str().unwrap().to_string();

                            let d = downloader.lock().await;
                            let config = d.config();
                            let url_text = config.url.to_string();
                            let chunks = d.get_chunks().await;
                            let chunks: Vec<ChunkWrapper> = chunks
                                .iter()
                                .map(|c| ChunkWrapper::from(c.clone()))
                                .collect();

                            info!(
                                "{} Progress: {} %，{}/{}",
                                file_name.clone(),
                                progress * 100 / total_len,
                                progress,
                                total_len
                            );

                            let wrapper = NalaiWrapper {
                                downloader: Some(downloader.clone()),
                                info: NalaiDownloadInfo {
                                    downloaded_bytes: progress,
                                    total_size: total_len,
                                    file_name: file_name,
                                    url: url_text,
                                    status: status_conversion::convert_status(
                                        status_conversion::DownloaderStatusWrapper::from(
                                            status_state.status(),
                                        ),
                                    ),
                                    speed: speed_state.download_speed(),
                                    save_dir: config.save_dir.to_str().unwrap().to_string(),
                                    create_time: original_info.create_time,
                                    chunks: chunks,
                                },
                            };

                            global_wrappers::insert_to_global_wrappers(id.clone(), wrapper).await;
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    // 实则是接收状态更新的说
                    while status_state.status_receiver.changed().await.is_ok() {
                        println!("State update: {:?}", status_state.status());
                        let progress = *downloaded_len_receiver.borrow();

                        if let Some(total_len) = total_len {
                            let full_path = downloader.lock().await.get_file_path();

                            let file_name =
                                full_path.file_name().unwrap().to_str().unwrap().to_string();

                            let d = downloader.lock().await;
                            let config = d.config();
                            let url_text = config.url.to_string();
                            let chunks = d.get_chunks().await;
                            let chunks = chunks
                                .iter()
                                .map(|c| ChunkWrapper::from(Arc::clone(c)))
                                .collect();

                            let wrapper = NalaiWrapper {
                                downloader: Some(downloader.clone()),
                                info: NalaiDownloadInfo {
                                    downloaded_bytes: progress,
                                    total_size: total_len,
                                    file_name: file_name,
                                    url: url_text,
                                    status: status_conversion::convert_status(
                                        DownloaderStatusWrapper::from(status_state.status()),
                                    ),
                                    speed: speed_state.download_speed(),
                                    save_dir: config.save_dir.to_str().unwrap().to_string(),
                                    create_time: original_info.create_time,
                                    chunks: chunks,
                                },
                            };

                            global_wrappers::insert_to_global_wrappers(id.clone(), wrapper).await;

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
                        global_wrappers::GLOBAL_WRAPPERS
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
                Ok(msg) => status_conversion::convert_status(msg),
                Err(msg) => status_conversion::convert_status(msg),
            };

            {
                let mut lock = global_wrappers::GLOBAL_WRAPPERS.lock().await;
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
pub async fn cancel_or_start_download_api(req: &mut Request, res: &mut Response) {
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
    info!("Cancel or start download，取消或开始下载");

    let info = match info::get_info(id).await {
        Some(it) => it,
        None => return Err(format!("No such download id: {}", id)),
    };

    let status = info.status.clone();

    let wrapper = match global_wrappers::get_wrapper_by_id(id).await {
        Some(it) => it,
        None => return Err(format!("No such download id: {}", id)),
    };

    match status {
        StatusWrapper::NoStart => {
            // 未开始下载，直接开始下载
            let url = Url::parse(&wrapper.info.url).unwrap();
            let save_dir = PathBuf::from(&wrapper.info.save_dir);
            let file_name = Some(wrapper.info.file_name.clone());

            start_download(&url, &save_dir, file_name, Some(id.to_string())).await;

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
            let file_name = Some(wrapper.info.file_name.clone());

            start_download(&url, &save_dir, file_name, Some(id.to_string())).await;

            Ok((true, true))
        }
        StatusWrapper::Finished => {
            // 下载完成，不做任何操作
            Ok((false, false))
        }
    }
}

async fn cancel_download(id: &str) -> anyhow::Result<bool, String> {
    info!("Cancel download，取消下载");

    let lock = global_wrappers::GLOBAL_WRAPPERS.lock().await;
    let wrapper = match lock.get(id) {
        Some(dl) => dl,
        None => return Err(format!("No such download id: {}", id)),
    };

    let downloader = wrapper.downloader.clone();
    if !downloader.is_none() {
        downloader.unwrap().lock().await.cancel().await;
    }

    match global_wrappers::save_all_to_file().await {
        Ok(it) => it,
        Err(err) => return Err(err.to_string()),
    };

    Ok(true)
}

pub async fn cancel_all_downloads() -> anyhow::Result<bool, String> {
    info!("Cancel all downloads，取消所有下载");

    let lock = global_wrappers::GLOBAL_WRAPPERS.lock().await;
    for (_, wrapper) in lock.iter() {
        if !wrapper.downloader.is_none() {
            if wrapper.info.status == StatusWrapper::Running
                || wrapper.info.status == StatusWrapper::Pending
            {
                let a = wrapper.downloader.as_ref().unwrap();
                a.lock().await.cancel().await;
            }
        }
    }

    match global_wrappers::save_all_to_file().await {
        Ok(it) => it,
        Err(err) => return Err(err.to_string()),
    };

    Ok(true)
}

pub async fn delete_download(id: &str) -> anyhow::Result<bool, String> {
    info!("Delete download，删除下载");

    cancel_download(id).await?;

    let value = global_wrappers::GLOBAL_WRAPPERS.lock().await.remove(id);

    let r = match value {
        Some(_) => {
            let a: Result<bool, String> = match global_wrappers::save_all_to_file().await {
                Ok(_) => Ok(true),
                Err(err) => return Err(err.to_string()),
            };
            a
        }
        None => Ok(false),
    };

    r
}
#[handler]
pub async fn delete_download_api(req: &mut Request, res: &mut Response) {
    let id = req.query::<String>("id").unwrap_or_default();

    match delete_download(&id).await {
        Ok(success) => {
            let all_info = info::get_all_info().await;
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
pub async fn cancel_download_api(req: &mut Request, res: &mut Response) {
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
