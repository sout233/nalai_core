use std::{collections::HashMap, path::PathBuf, sync::Arc};

use crate::{handlers::info, models::{nalai_download_info::NalaiDownloadInfo, nalai_wrapper::NalaiWrapper}};
use once_cell::sync::Lazy;
use tokio::sync::Mutex;
use tracing::info;

pub(crate) static GLOBAL_WRAPPERS: Lazy<Arc<Mutex<HashMap<String, NalaiWrapper>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

pub(crate) async fn load_global_wrappers_from_json() {
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
