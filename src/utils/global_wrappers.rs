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

    let db = match sled::open(file_path){
        Ok(db) => db,
        Err(e) => {
            info!("打开sled数据库失败: {}", e);
            info!("正在备份原始数据库到nalai_info_data.sled.bak");
            std::fs::rename("nalai_info_data.sled", "nalai_info_data.sled.bak").expect("备份失败");
            info!("备份完成，正在重新创建数据库");
            sled::open("nalai_info_data.sled").unwrap()
        }
    };


    let all_info: HashMap<String, NalaiDownloadInfo> = db.iter()
    .filter_map(|result| result.ok()) // 处理可能的错误，多过滤一次以省略无效数据（大聪明）
    .filter_map(|(k, v)| {
        let id = String::from_utf8(k.to_vec()).ok()?;
        let info: NalaiDownloadInfo = serde_json::from_slice(&v).ok()?;
        Some((id, info))
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
    info!("数据已从sled数据库加载");
}


pub async fn save_all_to_sled(should_flush: bool) -> anyhow::Result<()> {
    let db = sled::open("nalai_info_data.sled")?;
    info!("开始保存数据到sled数据库");
    let all_info = info::get_all_info().await;
    info!("获取数据成功");
    for (id, info) in all_info.iter() {
        let info_bytes = serde_json::to_vec(&info)?;
        db.insert(id.as_bytes(), info_bytes)?;
    }
    info!("数据已序列化");
    if should_flush {
        db.flush()?;
    }
    info!("数据已保存到sled数据库");
    Ok(())
}