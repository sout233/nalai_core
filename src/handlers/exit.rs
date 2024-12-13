use crate::{handlers::download, models::nalai_result::NalaiResult, utils::global_wrappers};
use salvo::prelude::*;
use serde_json::{json, Value};
use tracing::info;

#[handler]
pub async fn exit_api(_req: &mut Request, res: &mut Response) {
    info!("收到中断信号，正在取消所有下载任务...");
    match download::cancel_all_downloads().await {
        Err(e) => {
            eprintln!("取消下载失败: {}", e);
        }
        _ => {
            info!("所有下载任务已取消");
        }
    }
    
    match global_wrappers::save_all_to_sled(true).await {
        Err(e) => {
            eprintln!("保存下载记录失败: {}", e);
            let result = NalaiResult::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                Some("Error saving download records to sled"),
                json!({"error":e.to_string()}),
            );
            res.render(Json(result));
        }
        _ => {
            info!("下载记录已保存");
            let result = NalaiResult::new(StatusCode::OK,Some("Exit signal received"), Value::Null);
            res.render(Json(result));
        }
    }

    std::process::exit(0);
}
