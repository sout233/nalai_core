use std::collections::HashMap;
use std::sync::Arc;

use salvo::prelude::*;
use serde_json::{to_value, Value};
use tracing::info;

use crate::models::nalai_download_info::NalaiDownloadInfo;
use crate::models::nalai_result::NalaiResult;
use crate::utils::global_wrappers;

pub(crate) async fn get_info(id: &str) -> Option<NalaiDownloadInfo> {
    let wrapper = global_wrappers::get_wrapper_by_id(id).await;

    if wrapper.is_none() {
        return None;
    }

    let info = wrapper.unwrap().info.clone();
    Some(info)
}

#[handler]
pub async fn get_info_api(req: &mut Request, res: &mut Response) {
    let id = req.query::<String>("id");

    match id {
        Some(id) => {
            // info!("Get status for id: {}", id);

            let info = match get_info(&id).await {
                Some(info) => info,
                None => {
                    let result = NalaiResult::new(StatusCode::NOT_FOUND,Some("No such id found") ,Value::Null);
                    res.render(Json(result));
                    return;
                }
            };

            let result = NalaiResult::new(StatusCode::OK,Some("Info found") ,to_value(info).unwrap());

            res.render(Json(result));
        }
        None => {
            let result = NalaiResult::new(StatusCode::BAD_REQUEST,Some("No id provided"), Value::Null);

            res.render(Json(result));
        }
    }
}

#[handler]
pub async fn get_all_info_api(_req: &mut Request, res: &mut Response) {
    // global_wrappers::save_all_to_file().await.unwrap();
    global_wrappers::save_all_to_sled(false).await.unwrap();
    let all_info = get_all_info().await;
    let result = NalaiResult::new( StatusCode::OK,Some("All info found"), to_value(all_info).unwrap());
    res.render(Json(result));
}

pub(crate) async fn get_all_info() -> HashMap<String, NalaiDownloadInfo> {
    info!("Get all info");

    let all_infos = Arc::clone(&global_wrappers::GLOBAL_WRAPPERS);
    let lock = all_infos.lock().await;
    info!("Got lock");
    let mut result = HashMap::new();
    for (id, wrapper) in lock.iter() {
        result.insert(id.clone(), wrapper.info.clone());
    }
    result
}
