#[handler]
async fn start_download(req: &mut Request) -> String {
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

    let download_future = downloader.prepare_download().unwrap();
    let file_path = downloader
        .get_file_path()
        .to_str()
        .unwrap_or_default()
        .to_string();
    let id = general_purpose::STANDARD.encode(file_path.as_bytes());

    // let _status = status_state.status(); // get download status， 获取状态
    let mut status_receiver = status_state.status_receiver; //status watcher，状态监听器

    // let _byte_per_second = speed_state.download_speed(); // get download speed，Byte per second，获取速度，字节每秒
    // let _speed_receiver = speed_state.receiver; // get download speed watcher，速度监听器

    let downloader = Arc::new(Mutex::new(downloader));

    GLOBAL_DOWNLOADERS
        .lock()
        .await
        .insert(id.clone(), downloader.clone());

    tokio::spawn({
        let id = id.clone();

        async move {
            while status_receiver.changed().await.is_ok() {
                let status = status_receiver.borrow().clone();

                let mut info = GLOBAL_STATUS_LIST
                    .lock()
                    .await
                    .get(&id)
                    .unwrap_or(&NalaiInfo::default())
                    .clone();

                info.status = format!("{:?}", status.clone());

                GLOBAL_STATUS_LIST
                    .lock()
                    .await
                    .insert(id.clone(), info.clone());

                info!(
                    "Insert status for: {:?}, status: {:?}, file_path: {}",
                    id, info, file_path
                );

                tokio::time::sleep(Duration::from_millis(1000)).await;
            }
        }
    });

    // 打印下载进度
    tokio::spawn({
        let mut downloaded_len_receiver = downloader.lock().await.downloaded_len_receiver().clone();
        let total_size_future = downloader.lock().await.total_size_future();
        let id = id.clone();

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

    let result = download_future.await.unwrap();

    format!("{:?}", result)
}