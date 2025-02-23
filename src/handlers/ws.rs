use std::{collections::HashMap, sync::atomic::AtomicUsize};

use once_cell::sync::Lazy;
use salvo::{
    prelude::*,
    websocket::{Message, WebSocket},
};
use serde_json::Value;
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info};
use futures_util::{FutureExt, StreamExt};
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::{
    handlers::info::{get_all_info, get_info},
    models::ws_query,
};

type Clients = RwLock<HashMap<usize, mpsc::UnboundedSender<Result<Message, salvo::Error>>>>;

static ONLINE_CLIENTS: Lazy<Clients> = Lazy::new(Clients::default);
static NEXT_CLIENT_ID: AtomicUsize = AtomicUsize::new(1);

#[handler]
pub async fn connect(req: &mut Request, res: &mut Response) -> Result<(), StatusError> {
    WebSocketUpgrade::new()
        .upgrade(
            req, res,
            handle_ws, //     |mut ws: salvo::websocket::WebSocket| async move {
                      //     while let Some(msg) = ws.recv().await {
                      //         let msg = msg.ok().unwrap();
                      //         // info!("Received message: {:?}", msg);
                      //         let id = NEXT_CLIENT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                      //         let (tx, _rx) = mpsc::unbounded_channel();
                      //         let mut lock = ONLINE_CLIENTS.write().await;
                      //         lock.insert(id, tx);
                      //         drop(lock);
                      //         info!("New client connected: {}", id);

                      //         if msg.is_text() {
                      //             let query = serde_json::from_str::<ws_query::WsQuery>(&msg.as_str().unwrap()).unwrap();
                      //             match query.kind.as_str() {
                      //                 "get_all_info" => {
                      //                     let all_info = get_all_info().await;
                      //                     let result = Message::text(serde_json::to_string(&all_info).unwrap());
                      //                     if ws.send(result).await.is_err() {
                      //                         info!("Client disconnected");
                      //                         return;
                      //                     }
                      //                 }
                      //                 "get_info" => {
                      //                     let info = get_info(query.data.as_ref().unwrap()).await.unwrap();
                      //                     let result = Message::text(serde_json::to_string(&info).unwrap());
                      //                     if ws.send(result).await.is_err() {
                      //                         info!("Client disconnected");
                      //                         return;
                      //                     }
                      //                 }
                      //                 _ => {
                      //                     let result = Message::text(format!("Unknown query: {}", query.kind));
                      //                     if ws.send(result).await.is_err() {
                      //                         info!("Client disconnected");
                      //                         return;
                      //                     }
                      //                 }
                      //             }
                      //         }
                      //     }
                      // }
        )
        .await
}

async fn handle_ws(mut ws: WebSocket) {
    let id = NEXT_CLIENT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let (user_ws_tx, mut user_ws_rx) = ws.split();
    let (tx, rx) = mpsc::unbounded_channel();
    let rx = UnboundedReceiverStream::new(rx);
    let fut = rx.forward(user_ws_tx).map(|result| {
        if let Err(e) = result {
            tracing::error!(error = ?e, "websocket send error");
        }
    });
    tokio::task::spawn(fut);

    let fut = async move{
        let mut lock = ONLINE_CLIENTS.write().await;
        lock.insert(id, tx);
        info!("New client connected: {}", id);
        drop(lock);
    };
    
    tokio::task::spawn(fut);

    // ws.send(Message::text("Welcome to Nalai!")).await.unwrap();

    // while let Some(msg) = ws.recv().await {
    //     let msg = msg.ok().unwrap();
    //     info!("Received message: {:?}", msg);
    //     if msg.is_close() {
    //         info!("Client {:?} disconnected", id);
    //         ONLINE_CLIENTS.write().await.remove(&id);
    //     }
    // }
}

pub async fn send_value_to_client(msg: &Value) {
    for (id, tx) in ONLINE_CLIENTS.read().await.iter() {
        info!("Sending message to client {}", id);
        if tx.send(Ok(Message::text(msg.to_string()))).is_err() {
            error!("Failed to send message to client {}", id);
            // ONLINE_CLIENTS.write().await.remove(id);
        }
    }
}
