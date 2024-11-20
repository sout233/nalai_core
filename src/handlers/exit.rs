use crate::{models::nalai_result::NalaiResult, utils::global_wrappers};
use salvo::prelude::*;
use serde_json::{json, Value};
use tracing::info;

#[handler]
pub async fn exit_api(_req: &mut Request, res: &mut Response) {
    info!("收到中断信号，正在保存数据...");
    if let Err(e) =global_wrappers::save_all_to_file().await {
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
