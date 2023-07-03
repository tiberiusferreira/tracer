use crate::StatusData;
use api_structs::exporter::TraceSummary;
pub use api_structs::exporter::{Span, Trace};
use parking_lot::RwLock;
use reqwest::Client;
use serde::Serialize;
use std::collections::vec_deque::Iter;
use std::collections::VecDeque;
use std::ops::DerefMut;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug)]
pub struct TracerExporter {
    collector_url: String,
    export_timeout: Duration,
    client: Client,
    export_queue: Arc<RwLock<ExportQueue>>,
    errors: Arc<RwLock<[Option<String>; 5]>>,
}

#[derive(Debug)]
struct ExportQueue {
    inner: VecDeque<Trace>,
}

impl ExportQueue {
    pub fn push(&mut self, trace: Trace) {
        self.inner.push_back(trace);
    }
    pub fn pop(&mut self) -> Option<Trace> {
        self.inner.pop_front()
    }
    fn get_queue(&self) -> Iter<Trace> {
        self.inner.iter()
    }
    fn new() -> Self {
        ExportQueue {
            inner: VecDeque::new(),
        }
    }
}

impl TracerExporter {
    async fn export<T: Serialize + ?Sized>(
        client: &Client,
        collector_url: &str,
        export_timeout: Duration,
        export_item: &T,
        errors: &RwLock<[Option<String>; 5]>,
    ) {
        let response = client
            .post(collector_url)
            .timeout(export_timeout)
            .json(export_item)
            .send()
            .await;
        match response {
            Ok(resp) => {
                if resp.status().as_u16() != 200 {
                    let status = resp.status().as_u16();
                    let body = resp
                        .text()
                        .await
                        .map(|str| str.chars().take(30).collect::<String>());
                    let e = format!(
                        "Error during export: Got {status} status back with body: {:#?}",
                        body
                    );
                    println!("{e}");
                    let mut errs = errors.write();
                    let empty_slot = errs.iter_mut().find(|e| e.is_none());
                    if let Some(empty_slot) = empty_slot {
                        *empty_slot = Some(e);
                    }
                }
            }
            Err(e) => {
                println!("Error during export: {:?}", e);
                let mut errs = errors.write();
                let empty_slot = errs.iter_mut().find(|e| e.is_none());
                if let Some(empty_slot) = empty_slot {
                    *empty_slot = Some(format!("{:#?}", e));
                }
            }
        }
    }
    pub fn new(collector_url: String, export_timeout: Duration) -> Self {
        let client = reqwest::ClientBuilder::new()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Couldn't create Reqwest client");
        let export_queue = Arc::new(RwLock::new(ExportQueue::new()));
        let errors = Arc::new(RwLock::new([None, None, None, None, None]));
        let export_task = {
            let queue = Arc::clone(&export_queue);
            let errors = Arc::clone(&errors);
            let client = client.clone();
            let collector_url = collector_url.clone();
            async move {
                loop {
                    while let Some(export_item) = {
                        let mut w_guard = queue.write();
                        let entry = w_guard.pop();
                        drop(w_guard);
                        entry
                    } {
                        Self::export(
                            &client,
                            &format!("{}/collector/trace", collector_url),
                            export_timeout,
                            &export_item,
                            &errors,
                        )
                        .await;
                    }
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            }
        };
        tokio::spawn(export_task);

        Self {
            collector_url,
            export_timeout,
            client,
            export_queue,
            errors,
        }
    }
}

#[async_trait::async_trait]
pub trait Exporter {
    fn add_to_queue(&self, trace: Trace);
    async fn export_status(&self, status: StatusData);
    fn get_queue_summary(&self) -> Vec<TraceSummary>;
    fn take_errors(&self) -> Vec<String>;
}

#[async_trait::async_trait]
impl Exporter for TracerExporter {
    fn add_to_queue(&self, trace: Trace) {
        self.export_queue.write().push(trace);
    }

    async fn export_status(&self, status: StatusData) {
        TracerExporter::export(
            &self.client,
            &format!("{}/collector/status", self.collector_url),
            self.export_timeout,
            &status,
            &self.errors,
        )
        .await;
    }

    fn get_queue_summary(&self) -> Vec<TraceSummary> {
        self.export_queue
            .read()
            .get_queue()
            .map(|q| q.summary())
            .collect()
    }

    fn take_errors(&self) -> Vec<String> {
        let errors = std::mem::take(self.errors.write().deref_mut());
        let errors = errors
            .into_iter()
            .filter_map(|e| e)
            .collect::<Vec<String>>();
        errors
    }
}
