use std::io::Write;
use std::time::Duration;
use tokio::sync::mpsc::{Receiver, Sender};
use crate::recording::global_recorder::get_global_recorder;

#[derive(Debug, Clone, thiserror::Error)]
pub enum FlushError {
    #[error("Error sending request, subscriber receiving channel is closed or blocked")]
    ChannelClosedBeforeSend,
    #[error("Timeout waiting for response")]
    Timeout,
    #[error("Subscriber receiver channel closed before we got a response")]
    ChannelClosedAfterSend,
    #[error("Data was sent, but we got an error back from Tracer Backend: {0}")]
    TracerBackend(String),
}

fn export_to_disk(root_dir_path: std::path::PathBuf) {
    let execution_recordings = get_global_recorder().get_all_pruning();
    if execution_recordings.is_empty() {
        println!("no execution recordings to export");
    }
    for e in execution_recordings {
        let recording_dir = root_dir_path.join(e.id.to_string());
        std::fs::create_dir_all(&recording_dir).expect("to be able to create directory");
        let file_path = recording_dir.join(format!("{}.json", e.last_seen_at.to_rfc3339()));
        let as_json = serde_json::to_string_pretty(&e).expect("to be able to serialize");
        let mut file = std::fs::File::create(file_path).expect("to be able to create directory");
        file.write_all(as_json.as_bytes()).expect("to be able to write to file");
    }
}


struct FlushRequest {
    respond_to: tokio::sync::oneshot::Sender<Result<(), String>>,
}

impl FlushRequest {
    fn new() -> (
        tokio::sync::oneshot::Receiver<Result<(), String>>,
        FlushRequest,
    ) {
        let (sender, receiver) = tokio::sync::oneshot::channel::<Result<(), String>>();
        (receiver, Self { respond_to: sender })
    }
}

/// Used for request immediate exporting of current data. This is useful for when the
/// program is about to exit or during tests
#[derive(Debug, Clone)]
pub struct ExportNowRequester {
    sender_channel: Sender<FlushRequest>,
}

impl Drop for ExportNowRequester {
    fn drop(&mut self) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .thread_name("tracer_disk_exporter")
            .build()
            .expect("runtime to be able to start");
        runtime.block_on(async move {
            self.export(Duration::from_secs(5)).await.unwrap();
        });
    }
}

pub struct TracerHandle {
    pub thread_handle: std::thread::JoinHandle<()>,
    pub export_now_requester: ExportNowRequester,
}


impl ExportNowRequester {
    fn new() -> (Receiver<FlushRequest>, ExportNowRequester) {
        let (sender, receiver) = tokio::sync::mpsc::channel::<FlushRequest>(1);
        (
            receiver,
            Self {
                sender_channel: sender,
            },
        )
    }
    pub fn try_export_dont_wait_result(&self) -> Result<(), FlushError> {
        let (_receiver, request) = FlushRequest::new();
        self.sender_channel
            .try_send(request)
            .map_err(|_e| FlushError::ChannelClosedBeforeSend)?;
        Ok(())
    }
    pub async fn export(&self, timeout: Duration) -> Result<(), FlushError> {
        let (receiver, request) = FlushRequest::new();
        self.sender_channel
            .try_send(request)
            .map_err(|_e| FlushError::ChannelClosedBeforeSend)?;
        let flush = tokio::time::timeout(timeout, receiver)
            .await
            .map_err(|_e| FlushError::Timeout)?
            .map_err(|_e| FlushError::ChannelClosedAfterSend)?
            .map_err(|e| FlushError::TracerBackend(e))?;
        Ok(flush)
    }
}

pub async fn setup_disk_exporter(path: &str) -> TracerHandle {
    let root_dir_path = std::path::Path::new(path).to_path_buf();

    let (mut export_now_request_receiver, export_now_request_sender) = ExportNowRequester::new();
    let thread_handle = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .thread_name("tracer_disk_exporter")
            .build()
            .expect("runtime to be able to start");
        runtime.block_on(async {
            loop {
                tokio::select! {
                    request = export_now_request_receiver.recv() => {
                        let request: Option<FlushRequest> = request;
                        match request {
                            None => {
                                // channel handle dropped
                                tokio::time::sleep(Duration::from_secs(10)).await;
                            }
                            Some(request) => {
                                println!("flushing recording to disk");
                                export_to_disk(root_dir_path.clone());
                                let _ = request.respond_to.send(Ok(()));
                            }
                        }

                    }
                    _ = tokio::time::sleep(Duration::from_secs(10)) => {
                        println!("regular export");
                        export_to_disk(root_dir_path.clone());
                    }
                }
            }
        })
    });
    TracerHandle {
        thread_handle,
        export_now_requester: export_now_request_sender,
    }
}