use salvo::{prelude::*, websocket::Message};
use tracing::info;

use crate::{handlers::info::{get_all_info, get_info}, models::ws_query};

#[handler]
pub async fn connect(req: &mut Request, res: &mut Response) -> Result<(), StatusError> {
    WebSocketUpgrade::new()
        .upgrade(req, res, |mut ws| async move {
            while let Some(msg) = ws.recv().await {
                let msg = msg.ok().unwrap();
                // info!("Received message: {:?}", msg);

                if msg.is_text() {
                    let query = serde_json::from_str::<ws_query::WsQuery>(&msg.as_str().unwrap()).unwrap();
                    match query.kind.as_str() {
                        "get_all_info" => {
                            let all_info = get_all_info().await;
                            let result = Message::text(serde_json::to_string(&all_info).unwrap());
                            if ws.send(result).await.is_err() {
                                info!("Client disconnected");
                                return;
                            }
                        }
                        "get_info" => {
                            let info = get_info(query.data.as_ref().unwrap()).await.unwrap();
                            let result = Message::text(serde_json::to_string(&info).unwrap());
                            if ws.send(result).await.is_err() {
                                info!("Client disconnected");
                                return;
                            }
                        }
                        _ => {
                            let result = Message::text(format!("Unknown query: {}", query.kind));
                            if ws.send(result).await.is_err() {
                                info!("Client disconnected");
                                return;
                            }
                        }
                    }
                }
            }
        })
        .await
}
