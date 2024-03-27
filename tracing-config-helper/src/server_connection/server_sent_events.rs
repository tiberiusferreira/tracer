use std::time::Duration;

use futures_util::StreamExt;
use reqwest_eventsource::Event;
use tracing_subscriber::reload::Handle;
use tracing_subscriber::{EnvFilter, Registry};

use api_structs::instance::connect::SseRequest;
use api_structs::InstanceId;

use crate::{print_if_dbg, SSE_CONNECT_ENDPOINT};

#[derive(Debug)]
pub enum Error {
    ConnectionFailed,
    HttpError(reqwest_eventsource::Error),
}

pub async fn continuously_listen_for_server_sent_events<OnMessage, Fut>(
    instance_id: InstanceId,
    collector_url: String,
    on_event: OnMessage,
) where
    OnMessage: Fn(Result<String, Error>) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let context = "continuously_handle_server_sent_events";
    loop {
        let base_url = format!("{collector_url}{}", SSE_CONNECT_ENDPOINT);
        let url = reqwest::Url::parse_with_params(
            &base_url,
            [
                ("name", instance_id.service_id.name.as_str()),
                ("env", instance_id.service_id.env.to_string().as_str()),
                ("instance_id", instance_id.instance_id.to_string().as_str()),
            ],
        )
        .unwrap_or_else(|e| {
            panic!(
                "{base_url} url and instance_id={instance_id:?} were not able to be parsed: {e:?}"
            )
        });

        print_if_dbg(context, format!("Starting sse loop, connecting to {url}"));
        let mut event_source = reqwest_eventsource::EventSource::get(url);
        print_if_dbg(context, "sse connected");
        while let Some(event) = event_source.next().await {
            match event {
                Ok(Event::Open) => {
                    print_if_dbg(context, "sse open event");
                }
                Ok(Event::Message(message)) => {
                    on_event(Ok(message.data)).await;
                }
                Err(err) => {
                    on_event(Err(Error::HttpError(err))).await;
                }
            }
        }
        on_event(Err(Error::ConnectionFailed));
        let sleep_time_s = 10;
        println!("{context} - Server Sent Events connection failed, retrying in: {sleep_time_s}s");
        tokio::time::sleep(Duration::from_secs(sleep_time_s)).await;
    }
}

async fn handle_new_sse_message(
    new_msg: Result<String, Error>,
    filter_reload_handle: &Handle<EnvFilter, Registry>,
) {
    let context = "continuously_handle_server_sent_events";

    match new_msg {
        Ok(message) => {
            print_if_dbg(context, format!("sse message: {:#?}", message));
            let request: SseRequest = match serde_json::from_str(&message) {
                Ok(sse_request) => sse_request,
                Err(e) => {
                    println!(
                        "{context} - Could not parse sse message: {:?} - msg: {}",
                        e, message
                    );
                    return;
                }
            };
            match request {
                SseRequest::NewFilter { filter } => {
                    let new_env_filter = match EnvFilter::try_new(&filter) {
                        Ok(new_env_filter) => new_env_filter,
                        Err(e) => {
                            println!(
                                "{context} - Could not create env filter using: {filter} {:?}",
                                e
                            );
                            return;
                        }
                    };
                    if let Err(e) = filter_reload_handle.reload(new_env_filter) {
                        println!("{context} - Failed to reload filters: {:?}", e);
                    }
                }
            }
        }
        Err(e) => {
            println!("{context} - SSE error: {e:#?}");
        }
    }
}
pub async fn continuously_handle_server_sent_events(
    instance_id: InstanceId,
    collector_url: String,
    filter_reload_handle: Handle<EnvFilter, Registry>,
) {
    let handler = move |new_msg: Result<String, Error>| {
        let filter_reload_handle = filter_reload_handle.clone();
        async move { handle_new_sse_message(new_msg, &filter_reload_handle).await }
    };
    continuously_listen_for_server_sent_events(instance_id, collector_url, handler).await;
}

#[cfg(test)]
mod test {
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use api_structs::{Env, InstanceId, ServiceId};

    use crate::server_connection::server_sent_events::continuously_listen_for_server_sent_events;
    use crate::SSE_CONNECT_ENDPOINT;

    #[tokio::test]
    async fn listen_for_server_sent_events_works() {
        crate::test::enable_logging_for_tests();

        let mock_server = MockServer::start().await;
        let first_see_event = "some data";
        let second_see_event = "some data2";
        let instance_name = "some_name";
        let instance_env = "local";
        let instance_id = 2;
        Mock::given(method("GET"))
            .and(path(SSE_CONNECT_ENDPOINT))
            .and(query_param("name", instance_name))
            .and(query_param("env", instance_env))
            .and(query_param("instance_id", instance_id.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                format!("data: {first_see_event}\n\ndata: {second_see_event}\n\n").as_bytes(),
                "text/event-stream",
            ))
            // Mounting the mock on the mock server - it's now effective!
            .mount(&mock_server)
            .await;
        let (s, mut r) = tokio::sync::mpsc::channel(5);
        let _sse_task = tokio::spawn(continuously_listen_for_server_sent_events(
            InstanceId {
                service_id: ServiceId {
                    name: instance_name.to_string(),
                    env: Env::from(instance_env.to_string()),
                },
                instance_id,
            },
            mock_server.uri(),
            move |e| {
                let s = s.clone();
                async move {
                    s.send(e).await.unwrap();
                }
            },
        ));
        let first_msg = r.recv().await.unwrap().unwrap();
        assert_eq!(first_msg, first_see_event);
        let second_msg = r.recv().await.unwrap().unwrap();
        assert_eq!(second_msg, second_see_event);
        let err = r.recv().await.unwrap();
        assert!(matches!(
            err.unwrap_err(),
            super::Error::HttpError(reqwest_eventsource::Error::StreamEnded)
        ));
        let _err = r.recv().await.unwrap();
        mock_server.verify().await;
    }
}
