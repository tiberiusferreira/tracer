use crate::exporter::{ApplicationToCollectorMessage, CollectorToApplicationMessage};
use crate::websocket::Error2;
use axum::async_trait;
use futures::future::BoxFuture;
use futures::stream::SplitSink;
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::pin::pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::RwLock;
pub use tokio_tungstenite::tungstenite::Error;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

#[derive(Clone)]
pub struct ReqResp {
    sender: Arc<RwLock<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>>,
    send_timeout: Duration,
    receive_timeout: Duration,
    handlers: Arc<
        RwLock<
            HashMap<
                String,
                tokio::sync::oneshot::Sender<
                    crate::websocket::Message<CollectorToApplicationMessage>,
                >,
            >,
        >,
    >,
}
impl ReqResp {
    pub fn new_tracer_client(
        send_timeout: Duration,
        receive_timeout: Duration,
        ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
        handler: Box<
            dyn Fn(
                    CollectorToApplicationMessage,
                ) -> BoxFuture<'static, ApplicationToCollectorMessage>
                + Send
                + Sync
                + 'static,
        >,
    ) -> (tokio::task::JoinHandle<()>, ReqResp) {
        let handlers: Arc<
            RwLock<
                HashMap<
                    String,
                    tokio::sync::oneshot::Sender<
                        crate::websocket::Message<CollectorToApplicationMessage>,
                    >,
                >,
            >,
        > = Arc::new(RwLock::new(HashMap::new()));
        let (sender, r) = ws.split();
        let handlers_closure = Arc::clone(&handlers);
        let stream_without_responses = r.filter_map(move |new_msg| {
            let handlers = Arc::clone(&handlers_closure);
            async move {
                let Ok(Message::Text(new_msg)) = new_msg else{
                        return None
                    };
                let msg: crate::websocket::Message<CollectorToApplicationMessage> =
                    serde_json::from_str(&new_msg).expect("Message to be serializable");
                if let Some(response_to) = &msg.response_to {
                    if let Some(listener_waiting_for_response) =
                        handlers.write().await.remove(response_to)
                    {
                        listener_waiting_for_response.send(msg).ok();
                        return None;
                    }
                }
                return Some(msg);
            }
        });
        let sender = Arc::new(RwLock::new(sender));
        let handler_sender = Arc::clone(&sender);
        let server_task = tokio::spawn(async move {
            let mut stream_without_responses = pin!(stream_without_responses);
            while let Some(msg) = stream_without_responses.next().await {
                let res = handler(msg.data.clone()).await;
                if let Err(e) = handler_sender
                    .write()
                    .await
                    .send(Message::Text(
                        serde_json::to_string(&msg.make_response(res)).unwrap(),
                    ))
                    .await
                {
                    println!("{:#?}", e);
                }
            }
        });

        (
            server_task,
            Self {
                sender,
                send_timeout,
                receive_timeout,
                handlers,
            },
        )
    }
}

#[async_trait]
impl crate::websocket::ReqRespSender<ApplicationToCollectorMessage, CollectorToApplicationMessage>
    for ReqResp
{
    async fn send(
        &mut self,
        msg: crate::websocket::Message<ApplicationToCollectorMessage>,
    ) -> Result<
        BoxFuture<Result<crate::websocket::Message<CollectorToApplicationMessage>, Error2>>,
        Error2,
    > {
        let id = msg.id.clone();
        let mut w_sender = self.sender.write().await;
        if let Err(e) = tokio::time::timeout(
            self.send_timeout,
            w_sender.send(Message::Text(serde_json::to_string(&msg).unwrap())),
        )
        .await
        .map_err(|_e| Error2::Timeout)?
        {
            return Err(Error2::Sending(e.to_string()));
        }
        drop(w_sender);
        let (s, r) = tokio::sync::oneshot::channel::<
            crate::websocket::Message<CollectorToApplicationMessage>,
        >();
        let mut w_guard = self.handlers.write().await;
        w_guard.retain(|_id, chan| !chan.is_closed());
        w_guard.insert(id, s);
        drop(w_guard);
        let result = async {
            tokio::time::timeout(self.receive_timeout, r)
                .await
                .map_err(|_e| Error2::Timeout)?
                .map_err(|_e| Error2::DisconnectedWaitingForResponse)
        };
        Ok(Box::pin(result))
    }
}
