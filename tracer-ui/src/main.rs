use leptos::logging::log;
use leptos::*;

mod grid;
mod logs;
use grid::TraceGrid;
mod details;
mod services;
mod summary;
use details::TraceDetails;
use leptos_router::*;
use logs::Logs;
use services::Services;
const API_SERVER_URL_NO_TRAILING_SLASH: &str = env!("API_SERVER_URL_NO_TRAILING_SLASH");

fn main() {
    _ = console_log::init();
    console_error_panic_hook::set_once();
    mount_to_body(|| view! {   <App/> });
    log!("Loaded up!");
}

#[component]
pub fn App() -> impl IntoView {
    let root_path = "/".to_string();
    view! {
        <>
            <header>
                <nav class="navigation">
                    <div class="navigation__button"></div>
                    <a class="navigation__button" href={&root_path}>"Trace Search"</a>
                    <a class="navigation__button" href=format!("{}summary", root_path)>"Summary"</a>
                    <a class="navigation__button" href=format!("{}services", root_path)>"Services Health"</a>
                    <a class="navigation__button" href=format!("{}logs", root_path)>"Logs"</a>
                </nav>
            </header>
                <Router>
                    <Routes>
                        <Route
                              path=root_path.clone()
                              view={
                                    let root_path= root_path.to_string();
                                    move ||{
                                        view! {
                                              <TraceGrid root_path=root_path.clone()/>
                                        }
                                    }
                                }
                            />
                        <Route
                              path=format!("{}trace", root_path)
                              view={
                                    let root_path= root_path.to_string();
                                    move || view! {
                                    <TraceDetails root_path=root_path.clone()/>
                                    }
                                }
                            />
                        <Route
                              path=format!("{}services", root_path)
                              view={
                                let root_path= root_path.to_string();
                                move || view! {

                                    <Services root_path=root_path.clone()/>
                                }
                              }
                            />
                        <Route
                              path=format!("{}logs", root_path)
                              view={
                                let root_path= root_path.to_string();
                                move || view! {
                                    <Logs root_path=root_path.clone()/>
                                }
                              }
                            />
                    </Routes>
                </Router>
        </>
    }
}

fn printable_local_date(timestamp: u64) -> String {
    let timestamp = i64::try_from(timestamp).unwrap();
    let nanos_in_1_sec = 1_000_000_000;
    let offset_minutes = js_sys::Date::new_0().get_timezone_offset() as i64;
    let timestamp = chrono::NaiveDateTime::from_timestamp_opt(
        timestamp / nanos_in_1_sec,
        u32::try_from(timestamp % nanos_in_1_sec).unwrap(),
    )
    .unwrap();
    crate::grid::utc_to_local_date(timestamp, offset_minutes)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

fn printable_local_date_ms(timestamp: u64) -> String {
    let timestamp = i64::try_from(timestamp).unwrap();
    let nanos_in_1_sec = 1_000_000_000;
    let offset_minutes = js_sys::Date::new_0().get_timezone_offset() as i64;
    let timestamp = chrono::NaiveDateTime::from_timestamp_opt(
        timestamp / nanos_in_1_sec,
        u32::try_from(timestamp % nanos_in_1_sec).unwrap(),
    )
    .unwrap();
    grid::utc_to_local_date(timestamp, offset_minutes)
        .format("%Y-%m-%d %H:%M:%.6f")
        .to_string()
}

fn secs_since(timestamp: u64) -> u64 {
    let timestamp_ms = js_sys::Date::now() as u64;
    let nanos_in_1_ms = 1_000_000;
    let nanos_in_1_sec = 1_000_000_000;
    let nanos = (timestamp_ms * nanos_in_1_ms)
        .checked_sub(timestamp)
        .unwrap();
    let secs = nanos / nanos_in_1_sec;
    secs
}
