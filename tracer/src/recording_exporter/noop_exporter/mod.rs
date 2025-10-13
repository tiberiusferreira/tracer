use std::time::Duration;
use crate::recording::global_recorder::get_global_recorder;

pub async fn setup_noop_exporter() {
    let _thread_handle = std::thread::spawn(move || {
        loop {
            let _execution_recordings = get_global_recorder().get_all_pruning();
            std::thread::sleep(Duration::from_secs(1));
        }
    });
}
