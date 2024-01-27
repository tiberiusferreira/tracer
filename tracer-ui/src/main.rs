use leptos::*;
use tracing::Level;
mod datetime;
mod grid;
mod orphan_events;
mod services;
mod trace;
use leptos_router::*;
use tracing_subscriber::fmt;
use tracing_subscriber_wasm::MakeConsoleWriter;
const API_SERVER_URL_NO_TRAILING_SLASH: &str = env!("API_SERVER_URL_NO_TRAILING_SLASH");
pub const PAGE_ROOT_URL: &str = "/";
pub const TRACE_BROWSER_PATH: &str = "trace/browser";
pub const TRACE_CHUNK_PATH: &str = "trace/chunk";
pub const ORPHAN_EVENTS_PATH: &str = "orphan_events";

fn main() {
    console_error_panic_hook::set_once();
    fmt()
        .with_max_level(Level::DEBUG)
        .with_writer(MakeConsoleWriter::default())
        .with_ansi(false)
        // For some reason, if we don't do this in the browser, we get
        // a runtime error.
        .without_time()
        .init();
    mount_to_body(|| view! {   <App/> });
}

#[component]
pub fn App() -> impl IntoView {
    view! {
        <>
            <header>
                <nav class="navigation">
                    <div class="navigation__button"></div>
                    <a class="navigation__button" href={PAGE_ROOT_URL}>"Services"</a>
                    <a class="navigation__button" href=format!("{PAGE_ROOT_URL}{TRACE_BROWSER_PATH}")>"Trace Browser"</a>
                    <a class="navigation__button" href=format!("{PAGE_ROOT_URL}{ORPHAN_EVENTS_PATH}")>"Orphan Events"</a>
                </nav>
            </header>
                <Router>
                    <Routes>
                        <Route
                            path=PAGE_ROOT_URL
                              view={
                                move || view! {
                                    <services::Services/>
                                }
                              }
                            />
                        <Route
                              path=format!("{PAGE_ROOT_URL}{TRACE_CHUNK_PATH}")
                              view={
                                    move || view! {
                                        <trace::TraceChunk/>
                                    }
                                }
                            />
                        <Route
                              path=format!("{PAGE_ROOT_URL}{TRACE_BROWSER_PATH}")
                              view={
                                    move ||{
                                        view! {
                                              <grid::TraceBrowser/>
                                        }
                                    }
                                }
                            />
                        <Route
                              path=format!("{PAGE_ROOT_URL}{ORPHAN_EVENTS_PATH}", )
                              view={
                                move || view! {
                                    <orphan_events::OrphanEvents/>
                                }
                              }
                            />
                    </Routes>
                </Router>
        </>
    }
}
