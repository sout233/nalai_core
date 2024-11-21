use salvo::prelude::*;
use serde_json::Value;

use crate::models::nalai_result::NalaiResult;

#[handler]
pub async fn check_health_api(_req: &mut Request, res: &mut Response) {
    let result = NalaiResult::new(true, StatusCode::OK, Value::Null);
    res.render(Json(result));
}