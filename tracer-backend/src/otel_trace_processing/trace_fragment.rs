use crate::otel_trace_processing::{estimate_size_bytes, OtelTraceId, PendingData, ServiceName};
use crate::proto_generated::opentelemetry::proto::trace::v1::Span as ProtoSpan;
use crate::{BYTES_IN_1MB, MAX_SINGLE_TRACE_SIZE_BYTES, MAX_TIME_WAIT_NEW_TRACE_DATA_SECONDS};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{error, info, instrument};

#[derive(Debug, Clone)]
pub struct SharedBuffer(Arc<RwLock<Buffer>>);

#[derive(Debug, Clone)]
pub struct Pusher(Arc<RwLock<Buffer>>);

#[derive(Debug, Clone)]
pub struct Popper(Arc<RwLock<Buffer>>);

impl Pusher {
    #[instrument(skip_all)]
    pub async fn try_push(
        &self,
        traces: HashMap<ServiceName, HashMap<OtelTraceId, Vec<ProtoSpan>>>,
    ) {
        let mut w_lock = self.0.write().await;
        for (service_name, trace_data) in traces {
            for (trace_id, spans) in trace_data {
                w_lock.try_add_new(service_name.to_string(), trace_id, spans);
            }
        }
    }
}

impl Popper {
    pub async fn pop_ready_for_processing(
        &self,
    ) -> HashMap<ServiceName, HashMap<OtelTraceId, Vec<ProtoSpan>>> {
        self.0.write().await.remove_entries_for_processing()
    }
}

impl SharedBuffer {
    fn split(self) -> (Pusher, Popper) {
        (Pusher(Arc::clone(&self.0)), Popper(Arc::clone(&self.0)))
    }
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> (Pusher, Popper) {
        SharedBuffer(Arc::new(RwLock::new(Buffer {
            traces: HashMap::default(),
        })))
        .split()
    }
}

#[derive(Debug, Clone)]
struct Buffer {
    traces: HashMap<ServiceName, HashMap<OtelTraceId, PendingData>>,
}

impl Buffer {
    fn is_full(&self) -> bool {
        let pending_traces_len = self.total_traces_len();
        u64::try_from(pending_traces_len).expect("usize to fit u64") >= crate::MAX_BUFFERED_TRACES
    }
    pub fn total_traces_len(&self) -> usize {
        let mut cnt = 0;
        for data in self.traces.values() {
            cnt += data.len();
        }
        cnt
    }
    pub fn remove_entries_for_processing(
        &mut self,
    ) -> HashMap<ServiceName, HashMap<OtelTraceId, Vec<ProtoSpan>>> {
        let mut traces_ready_for_processing: HashMap<
            ServiceName,
            HashMap<OtelTraceId, Vec<ProtoSpan>>,
        > = HashMap::new();
        for (service, traces) in &mut self.traces {
            let mut traces_to_remove = vec![];
            for (trace, data) in &*traces {
                if data.last_data_received_at.elapsed().as_secs()
                    > MAX_TIME_WAIT_NEW_TRACE_DATA_SECONDS
                {
                    traces_to_remove.push(trace.to_string());
                }
            }
            for t in traces_to_remove {
                let trace_data = traces.remove(&t).expect("key to exist");
                if trace_data.dropped_over_size_limit {
                    continue;
                }
                traces_ready_for_processing
                    .entry(service.to_string())
                    .or_default()
                    .insert(t, trace_data.spans);
            }
        }
        traces_ready_for_processing
    }
    #[instrument(skip_all)]
    pub fn try_add_new(&mut self, service_name: String, trace_id: String, spans: Vec<ProtoSpan>) {
        if self.is_full() {
            error!(
                "PendingServiceTraces hit the limit of {} traces, dropping new traces",
                crate::MAX_BUFFERED_TRACES
            );
            return;
        }
        let initial_len = self.total_traces_len();
        let now = Instant::now();
        let existing_spans = self
            .traces
            .entry(service_name.clone())
            .or_default()
            .entry(trace_id)
            .or_insert(PendingData {
                first_data_received_at: now,
                last_data_received_at: now,
                dropped_over_size_limit: false,
                spans: vec![],
            });
        existing_spans.last_data_received_at = now;
        if existing_spans.dropped_over_size_limit {
            info!("Got more data for an already dropped span, ignoring it");
            return;
        }
        let is_new_trace = existing_spans.first_data_received_at == now;
        existing_spans.spans.extend_from_slice(&spans);
        let size_bytes = estimate_size_bytes(&existing_spans.spans);
        let size_mb = size_bytes as f32 / BYTES_IN_1MB as f32;
        if size_bytes >= MAX_SINGLE_TRACE_SIZE_BYTES {
            error!(
                "Service {service_name} sent trace bigger than max size: {:.2} MB. Dropping it.",
                size_mb
            );
            existing_spans.spans = vec![];
            existing_spans.dropped_over_size_limit = true;
        } else if is_new_trace {
            info!(
                "Got new trace from: {service_name} estimated size: {:.2} MB - {} in buffer",
                size_mb,
                initial_len + 1
            );
        } else {
            info!(
                    "Got additional trace data from {} after {}ms - estimated total buffer size in use: {:.2} MB",
                    service_name,
                    existing_spans.first_data_received_at.elapsed().as_millis(),
                    size_mb
                );
        }
    }
}
