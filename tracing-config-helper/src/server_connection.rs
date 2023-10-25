use crate::print_if_dbg;
use api_structs::sse::SseRequest;
use futures_util::StreamExt;
use reqwest_eventsource::Event;
use std::time::Duration;
use tracing_subscriber::reload::Handle;
use tracing_subscriber::{EnvFilter, Registry};

pub trait ToEnvFilter {
    fn to_env_filter(&self) -> Result<EnvFilter, String>;
}
impl ToEnvFilter for api_structs::exporter::TracerFilters {
    fn to_env_filter(&self) -> Result<EnvFilter, String> {
        let desired_filter_as_str = self.to_filter_str();
        let filter = EnvFilter::builder().parse(&desired_filter_as_str).unwrap();
        // let obtained_filter_as_str = filter.to_string();
        // if desired_filter_as_str == obtained_filter_as_str {
        Ok(filter)
        // } else {
        //     Err(format!("Desired and obtained filters don't match: {desired_filter_as_str} vs {obtained_filter_as_str}"))
        // }
    }
}

pub async fn continuously_handle_server_sent_events(
    collector_url: String,
    filter_reload_handle: Handle<EnvFilter, Registry>,
    client_id: i64,
) {
    let context = "continuously_handle_server_sent_events";
    loop {
        print_if_dbg(context, "Starting sse loop");
        let mut event_source =
            reqwest_eventsource::EventSource::get(format!("{collector_url}/see/{client_id}"));
        print_if_dbg(context, "sse connected");
        while let Some(event) = event_source.next().await {
            match event {
                Ok(Event::Open) => {
                    print_if_dbg(context, "sse open event");
                }
                Ok(Event::Message(message)) => {
                    print_if_dbg(context, format!("sse message: {:#?}", message));
                    let request: SseRequest = match serde_json::from_str(&message.data) {
                        Ok(sse_request) => sse_request,
                        Err(e) => {
                            println!(
                                "{context} - Could not parse sse message: {:?} - msg: {}",
                                e, message.data
                            );
                            continue;
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
                                    continue;
                                }
                            };
                            if let Err(e) = filter_reload_handle.reload(new_env_filter) {
                                println!("{context} - Failed to reload filters: {:?}", e);
                            }
                        }
                    }
                }
                Err(err) => {
                    println!("{context} - sse error: {:?}", err);
                }
            }
        }
        let sleep_time_s = 10;
        println!("{context} - Server Sent Events connection failed, retrying in: {sleep_time_s}s");
        tokio::time::sleep(Duration::from_secs(sleep_time_s)).await;
    }
}
