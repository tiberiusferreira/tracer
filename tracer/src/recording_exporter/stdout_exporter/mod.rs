use std::time::Duration;
use crate::recording::global_recorder::get_global_recorder;

pub async fn setup_stdout_exporter() {
    let _thread_handle = std::thread::spawn(move || {
        loop {
            let execution_recordings = get_global_recorder().get_all_pruning();
            for e in execution_recordings{
                for (io_recorder_name, events) in e.execution_io_fragment.io_providers_events{
                    for e in events{
                        let pretty = serde_json::to_string_pretty(&e.value).expect("to be serializable");
                        eprintln!("{io_recorder_name}: {pretty}");
                    }
                }
                for (attr, value) in e.attributes {
                    eprintln!("{attr}: {value:?}");
                }
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    });
}
