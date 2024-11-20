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
use utils::global_wrappers;
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

mod handlers;
mod models;
mod utils;


#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    info!("Starting server");

    global_wrappers::load_global_wrappers_from_json().await;

    tokio::spawn(async {
        let router = Router::new()
            .push(
                Router::with_path("/download")
                    .post(handlers::download::start_download_api)
                    .delete(handlers::download::delete_download_api),
            )
            .push(Router::with_path("/info").get(handlers::info::get_info_api))
            .push(Router::with_path("/cancel").post(handlers::download::cancel_download_api))
            .push(Router::with_path("/all_info").get(handlers::info::get_all_info_api))
            .push(Router::with_path("/sorc").post(handlers::download::cancel_or_start_download_api))
            .push(Router::with_path("checkhealth").get(handlers::health::check_health_api))
            .push(Router::with_path("exit").get(handlers::exit::exit_api));

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
