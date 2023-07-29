use axum::async_trait;
use futures::future::BoxFuture;

pub mod axum_adapter;
pub mod client_adapter;

#[async_trait]
pub trait ReqRespSender<Req, Resp> {
    async fn send(
        &mut self,
        msg: Message<Req>,
    ) -> Result<BoxFuture<Result<Message<Resp>, Error2>>, Error2>;
}

pub struct RequestResponse;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Message<T> {
    id: String,
    response_to: Option<String>,
    pub data: T,
}

#[derive(Debug)]
pub enum Error2 {
    Timeout,
    Sending(String),
    DisconnectedWaitingForResponse,
    Other(String),
}

impl<T> Message<T> {
    pub fn new(data: T) -> Self {
        Message {
            id: uuid::Uuid::new_v4().to_string(),
            response_to: None,
            data,
        }
    }
    pub fn make_response<W>(&self, data: W) -> Message<W> {
        Message {
            id: uuid::Uuid::new_v4().to_string(),
            response_to: Some(self.id.clone()),
            data,
        }
    }
}
