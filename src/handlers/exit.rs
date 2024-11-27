use crate::{handlers::download, models::nalai_result::NalaiResult};
use salvo::prelude::*;
use serde_json::{json, Value};
use tracing::info;

#[handler]
pub async fn exit_api(_req: &mut Request, res: &mut Response) {
    info!("收到中断信号，正在取消所有下载任务...");
    if let Err(e) = download::cancel_all_downloads().await {
        eprintln!("取消下载失败: {}", e);
        let result = NalaiResult::new(
            false,
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"error":e.to_string()}),
        );
        res.render(Json(result));
    } else {
        info!("所有下载任务已取消");
        let result = NalaiResult::new(true, StatusCode::OK, Value::Null);
        res.render(Json(result));
        std::process::exit(0);
    }
}
