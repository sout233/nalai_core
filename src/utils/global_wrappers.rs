use std::{collections::HashMap, path::PathBuf, sync::Arc};

use crate::{handlers::info, models::{nalai_download_info::NalaiDownloadInfo, nalai_wrapper::NalaiWrapper}};
use once_cell::sync::Lazy;
use tokio::sync::Mutex;
use tracing::info;

pub(crate) static GLOBAL_WRAPPERS: Lazy<Arc<Mutex<HashMap<String, NalaiWrapper>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

#[deprecated]
#[allow(dead_code)]
pub(crate) async fn load_global_wrappers_from_json() {
    let file_path = PathBuf::from("nalai_info_data.json");
    if file_path.exists() {
        let json_str = std::fs::read_to_string(file_path).unwrap_or_default();
        let all_info: HashMap<String, NalaiDownloadInfo> = serde_json::from_str(&json_str).unwrap_or(HashMap::new());
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

#[deprecated]
#[allow(dead_code)]
pub(crate) async fn save_all_to_file() -> anyhow::Result<()> {
    tokio::spawn(async {
        let all_info = info::get_all_info().await;
        let json_str = serde_json::to_string(&all_info).unwrap();
        let file_path = PathBuf::from("nalai_info_data.json");
        tokio::fs::write(file_path.clone(), json_str).await.unwrap();
        info!("数据已保存到文件: {}", file_path.to_str().unwrap());
    });
    Ok(())
}
pub(crate) async fn get_wrapper_by_id(id: &str) -> Option<NalaiWrapper> {
    let lock = GLOBAL_WRAPPERS.lock().await;
    lock.get(id).cloned()
}

pub(crate) async fn insert_to_global_wrappers(id: String, wrapper: NalaiWrapper) {
    let mut global_wrappers = GLOBAL_WRAPPERS.lock().await;
    global_wrappers.insert(id, wrapper);

    // let _ = save_all_to_file().await;
}

// 一些重写的方法，从json换为sled数据库

pub(crate) async fn load_global_wrappers_from_sled() {
    let file_path = PathBuf::from("nalai_info_data.sled");
    if !file_path.exists() {
        return;
    }

    let db = sled::open(file_path).unwrap();

    let all_info: HashMap<String, NalaiDownloadInfo> = db.iter()
    .filter_map(|result| result.ok()) // 处理可能的错误
    .map(|(k, v)| {
        let id = String::from_utf8(k.to_vec()).expect("Invalid UTF-8 sequence");
        let info: NalaiDownloadInfo = serde_json::from_slice(&v).expect("Failed to deserialize NalaiDownloadInfo");
        (id, info)
    })
    .collect();

    let mut lock = GLOBAL_WRAPPERS.lock().await;
    for (id, info) in all_info.iter() {
        let wrapper = NalaiWrapper {
            downloader: None,
            info: info.clone(),
        };
        lock.insert(id.clone(), wrapper);
    }
}


pub async fn save_all_to_sled() -> anyhow::Result<()> {
    let db = sled::open("nalai_info_data.sled").unwrap();
    let all_info = info::get_all_info().await;
    for (id, info) in all_info.iter() {
        let info_bytes = serde_json::to_vec(&info).unwrap();
        db.insert(id.as_bytes(), info_bytes).unwrap();
    }
    Ok(())
}