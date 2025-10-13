//! This serves as a unified config for projects
//! It outputs pretty logs to the console stdout and stderr,
//! but also exports traces to a collector
//!

mod recording;
mod playing;


mod recording_exporter;
pub fn is_playing_recording() -> bool {
    std::env::var("GLOBAL_RECORDING_PATH".to_string()).is_ok()
}

pub mod application_api {
    pub use crate::recording::global_recorder::{record_execution, record_execution_simple, record_attribute, record_error};
}

pub mod recorder_api {
    pub use crate::recording::global_recorder::{get_global_recorder};
    pub use crate::recording::{record_io_event_request_or_panic, IoEventRequest};
}

pub mod player_api {
    pub use crate::playing::*;
}

pub mod server_exporter {
    pub use crate::recording_exporter::server_exporter::{ServerExporterConfig, setup_server_exporter_task_or_panic, ExportNowRequester, ServiceId};
}

pub mod noop_exporter {
    pub use crate::recording_exporter::noop_exporter::setup_noop_exporter;
}

pub mod disk_exporter {
    pub use crate::recording_exporter::disk_exporter::setup_disk_exporter;
}