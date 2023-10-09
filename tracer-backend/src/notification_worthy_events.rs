// // use crate::otel_trace_processing::{InsertedTrace, ServiceName};
// use serde::Serialize;
// use std::collections::HashMap;
// use std::ops::DerefMut;
// use std::str::FromStr;
// use std::sync::{Arc, OnceLock};
// use std::time::Duration;
// use tokio::sync::RwLock;
// use tokio::task::JoinHandle;
// use tracing::{error, info, instrument};
//
// const MAX_ENTRIES_PER_SERVICE: usize = 1_00;
// const MAX_SAMPLES_IN_MESSAGE: usize = 10;
//
// #[derive(Debug, Clone)]
// pub struct TraceInvalidationCause(String);
//
// impl TraceInvalidationCause {
//     pub fn from_cause(cause: &str) -> Self {
//         Self(cause.to_string())
//     }
// }
//
// pub type Shared<T> = Arc<RwLock<T>>;
// pub type TopLevelSpanName = String;
//
// #[derive(Debug, Clone)]
// pub struct Stats {
//     total_traces: usize,
//     total_span_plus_events: usize,
//     warning_count: usize,
//     error_count: usize,
// }
//
// // #[derive(Debug, Clone)]
// // pub struct NotificationWorthyEventsPusher {
// //     traces_with_errors: Shared<HashMap<ServiceName, Vec<InsertedTrace>>>,
// //     malformed_traces: Shared<HashMap<ServiceName, Vec<TraceInvalidationCause>>>,
// //     trace_stats: Shared<HashMap<ServiceName, HashMap<TopLevelSpanName, Stats>>>,
// // }
//
// impl NotificationWorthyEventsPusher {
//     pub async fn push_trace_with_error(&self, service_name: String, trace: InsertedTrace) {
//         let mut w_lock = self.traces_with_errors.write().await;
//         let existing_entries = w_lock.entry(service_name).or_default();
//         if existing_entries.len() < MAX_ENTRIES_PER_SERVICE {
//             existing_entries.push(trace);
//         }
//     }
//     pub async fn push_invalid_traces(&self, service_name: String, cause: TraceInvalidationCause) {
//         let mut w_lock = self.malformed_traces.write().await;
//         let existing_entries = w_lock.entry(service_name).or_default();
//         if existing_entries.len() < MAX_ENTRIES_PER_SERVICE {
//             existing_entries.push(cause);
//         }
//     }
//
//     pub async fn update_stats(
//         &self,
//         service_name: String,
//         top_level_span_name: String,
//         has_errors: bool,
//         has_warnings: bool,
//         span_plus_events_count: usize,
//     ) {
//         if has_errors || has_warnings {
//             let mut w_lock = self.trace_stats.write().await;
//             let service_entries = w_lock.entry(service_name).or_default();
//             let top_level_span_stats =
//                 service_entries.entry(top_level_span_name).or_insert(Stats {
//                     total_traces: 0,
//                     total_span_plus_events: 0,
//                     warning_count: 0,
//                     error_count: 0,
//                 });
//             top_level_span_stats.total_traces += 1;
//             top_level_span_stats.total_span_plus_events += span_plus_events_count;
//             if has_errors {
//                 top_level_span_stats.error_count += 1;
//             }
//             if has_warnings {
//                 top_level_span_stats.warning_count += 1;
//             }
//         }
//     }
// }
//
// pub struct Notifier {
//     slack_messenger: SlackNotifier,
//     traces_with_errors: Shared<HashMap<ServiceName, Vec<InsertedTrace>>>,
//     invalid_traces: Shared<HashMap<ServiceName, Vec<TraceInvalidationCause>>>,
//     trace_stats: Shared<HashMap<ServiceName, HashMap<TopLevelSpanName, Stats>>>,
//     time_between_runs: Duration,
//     span_plus_events_per_service_per_second_notification_threshold: usize,
// }
//
// impl Notifier {
//     #[instrument(skip_all)]
//     pub fn initialize_and_start_notification_task(
//         webhook_url: String,
//         time_between_runs: Duration,
//         span_plus_events_per_service_per_second_notification_threshold: usize,
//     ) -> (NotificationWorthyEventsPusher, JoinHandle<()>) {
//         static CELL: OnceLock<bool> = OnceLock::new();
//         let notifier = match CELL.set(true) {
//             Ok(()) => Self {
//                 slack_messenger: SlackNotifier::new(webhook_url),
//                 traces_with_errors: Arc::new(RwLock::new(HashMap::new())),
//                 invalid_traces: Arc::new(RwLock::new(HashMap::new())),
//                 trace_stats: Arc::new(RwLock::new(HashMap::new())),
//                 time_between_runs,
//                 span_plus_events_per_service_per_second_notification_threshold,
//             },
//             Err(_e) => panic!("Tried to initialize notification_worthy_events::Notifier twice"),
//         };
//         let pusher = notifier.pusher();
//         info!("Starting notifier task");
//         let task_handle = tokio::task::spawn(async move {
//             let mut err_notifier = notifier;
//             loop {
//                 err_notifier.consume_errors_sending_notifications().await;
//                 tokio::time::sleep(time_between_runs).await;
//             }
//         });
//         (pusher, task_handle)
//     }
//     fn pusher(&self) -> NotificationWorthyEventsPusher {
//         NotificationWorthyEventsPusher {
//             traces_with_errors: Arc::clone(&self.traces_with_errors),
//             malformed_traces: Arc::clone(&self.invalid_traces),
//             trace_stats: Arc::clone(&self.trace_stats),
//         }
//     }
//     #[instrument(skip_all)]
//     async fn consume_errors_sending_notifications(&mut self) {
//         let trace_stats = std::mem::take(self.trace_stats.write().await.deref_mut());
//         let mut message_lines_to_send = vec![];
//         for (service, service_stats) in trace_stats {
//             let mut service_total_span_plus_events = 0;
//             for stats in service_stats.values() {
//                 service_total_span_plus_events += stats.total_span_plus_events;
//             }
//             let time_between_runs_sec =
//                 usize::try_from(self.time_between_runs.as_secs()).expect("u64 to fit usize");
//             let service_total_span_plus_events_per_second = service_total_span_plus_events
//                 .checked_div(time_between_runs_sec)
//                 .unwrap_or(0);
//             if service_total_span_plus_events_per_second
//                 >= self.span_plus_events_per_service_per_second_notification_threshold
//             {
//                 let mut service_message_lines_to_send = vec![];
//                 let mut top_spans: Vec<(String, Stats)> = service_stats.into_iter().collect();
//                 top_spans.sort_by_key(|k| k.1.total_span_plus_events);
//                 top_spans.reverse();
//                 let top_3_spans: Vec<(String, Stats)> = top_spans.into_iter().take(3).collect();
//                 service_message_lines_to_send.push(format!("Service {service} sent {service_total_span_plus_events} span+events ({service_total_span_plus_events_per_second}/s). Top spans:"));
//                 let top_spans_message: Vec<String> = top_3_spans
//                     .into_iter()
//                     .map(|(top_lvl_span, stats)| {
//                         format!(
//                             "{top_lvl_span} {} ({}/s)",
//                             stats.total_span_plus_events,
//                             stats
//                                 .total_span_plus_events
//                                 .checked_div(time_between_runs_sec)
//                                 .unwrap_or(0)
//                         )
//                     })
//                     .collect();
//                 service_message_lines_to_send.extend_from_slice(&top_spans_message);
//                 let full_msg = service_message_lines_to_send.join("\n    ");
//                 message_lines_to_send.push(full_msg);
//             }
//         }
//         let invalid_traces = std::mem::take(self.invalid_traces.write().await.deref_mut());
//         for (service_name, causes) in invalid_traces {
//             let count = causes.len();
//             let samples = causes
//                 .into_iter()
//                 .take(MAX_SAMPLES_IN_MESSAGE)
//                 .map(|e| e.0)
//                 .collect::<Vec<String>>();
//             let samples = samples.join("\n    ");
//             message_lines_to_send.push(format!(
//                 "{service_name} sent invalid traces ({count}). Sample causes:\n    {samples}"
//             ));
//         }
//         let traces_with_error = std::mem::take(self.traces_with_errors.write().await.deref_mut());
//         for (service_name, trace_with_error) in traces_with_error {
//             let count = trace_with_error.len();
//             let err: String = trace_with_error
//                 .iter()
//                 .take(MAX_SAMPLES_IN_MESSAGE)
//                 .map(|e| {
//                     let id = &e.id;
//                     let top_level_span_name = &e.top_level_span_name;
//                     let frontend_url = api_structs::FRONTEND_PUBLIC_URL_PATH_NO_TRAILING_SLASH;
//                     format!("<{frontend_url}/trace?trace_id={id}|{top_level_span_name}>")
//                 })
//                 .collect::<Vec<String>>()
//                 .join(" ");
//             message_lines_to_send.push(format!(
//                 "{service_name} had errors ({count}). Samples: {err}"
//             ));
//         }
//         if !message_lines_to_send.is_empty() {
//             self.slack_messenger
//                 .send_slack_msg(&message_lines_to_send.join("\n"))
//                 .await;
//         } else {
//             info!("Notifier has no events to send");
//         }
//     }
// }
//
// #[derive(Clone, Debug, Serialize)]
// struct SlackMessage {
//     text: String,
// }
//
// struct SlackNotifier {
//     client: reqwest::Client,
//     webhook_url: reqwest::Url,
// }
//
// impl SlackNotifier {
//     pub fn new(webhook_url: String) -> Self {
//         Self {
//             client: reqwest::ClientBuilder::new()
//                 .timeout(Duration::from_secs(30))
//                 .build()
//                 .expect("Couldn't create Reqwest client"),
//             webhook_url: reqwest::Url::from_str(&webhook_url)
//                 .unwrap_or_else(|e| panic!("invalid slack url: {webhook_url}\n{e:?}")),
//         }
//     }
// }
// impl SlackNotifier {
//     #[instrument(skip_all)]
//     async fn send_slack_msg(&self, msg: &str) {
//         info!("Sending a Slack notification: {msg}");
//         let response = self
//             .client
//             .request(reqwest::Method::POST, self.webhook_url.clone())
//             .body(
//                 serde_json::to_string(&SlackMessage {
//                     text: msg.to_string(),
//                 })
//                 .expect("Slack msg was not serializable"),
//             )
//             .send()
//             .await;
//
//         match response {
//             Ok(request_response) => {
//                 // got response
//                 let status = request_response.status();
//                 if status.is_success() {
//                     info!("Sent a Slack notification");
//                 } else {
//                     error!(
//                         "Error sending Slack notification. HTTP Status: {:?}",
//                         status
//                     );
//                 }
//             }
//             // timed out
//             Err(err) => {
//                 error!("Error sending Slack notification: {}", err.to_string());
//             }
//         }
//     }
// }
