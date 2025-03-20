use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use tracing::Level;

pub mod datetime;
// mod grid;
// mod orphan_events;
mod alerts;
mod chart;
mod dashboard;
mod services;
mod trace;

use tracing_subscriber::fmt;
use tracing_subscriber_wasm::MakeConsoleWriter;

const API_SERVER_URL_NO_TRAILING_SLASH: &str = env!("API_SERVER_URL_NO_TRAILING_SLASH");
pub const PAGE_ROOT_URL: &str = "/";
pub const TRACE_BROWSER_PATH: &str = "trace/browser";
pub const TRACE_CHUNK_PATH: &str = "trace/chunk";
pub const ORPHAN_EVENTS_PATH: &str = "orphan_events";
pub const DASHBOARD_PATH: &str = "dashboard";
pub const ALERTS_PATH: &str = "alerts";

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
    mount_to_body(|| view! { <App /> });
}

#[component]
pub fn App() -> impl IntoView {
    view! {
        <>
            <Router>
                <header>
                    <nav class="navigation">
                        // just a spacer
                        <div class="navigation__button"></div>
                        <a class="navigation__button" href=PAGE_ROOT_URL>
                            "Services"
                        </a>
                        <a
                            class="navigation__button"
                            href=format!("{PAGE_ROOT_URL}{ALERTS_PATH}")
                        >
                            "Alerts"
                        </a>
                        <a
                            class="navigation__button"
                            href=format!("{PAGE_ROOT_URL}{TRACE_BROWSER_PATH}")
                        >
                            "Trace Browser"
                        </a>
                        <a
                            class="navigation__button"
                            href=format!("{PAGE_ROOT_URL}{ORPHAN_EVENTS_PATH}")
                        >
                            "Orphan Events"
                        </a>
                    </nav>
                </header>
                <Routes fallback=|| view!{<p style="color: white">"Not found."</p>} >
                    <Route path=leptos_router::StaticSegment("/") view=services::Services />
                    <Route path=(
                        leptos_router::StaticSegment("/"),
                        leptos_router::StaticSegment(ALERTS_PATH)
                    )
                        view=alerts::Alerts
                    />
                    <Route path=(
                        leptos_router::StaticSegment("/"),
                        leptos_router::StaticSegment(DASHBOARD_PATH)
                    )
                        view=dashboard::Dashboard
                    />
                    <Route path=(
                        leptos_router::StaticSegment("/"),
                        leptos_router::StaticSegment("trace"),
                        leptos_router::StaticSegment("browser")
                    )
                        view=trace::TraceBrowserPage
                    />
                </Routes>

                // </Routes>
                    // <ParentRoute
                    //       path=path!("")
                    //       view=view! {"dawda"}
                    //     >
                    // </ParentRoute>
            </Router>
        </>
    }
}

#[derive(Clone, Debug, thiserror::Error)]
#[error("TrackedGlooError at {location}")]
pub struct TrackedGlooError {
    location: &'static std::panic::Location<'static>,
    #[source]
    source: std::sync::Arc<gloo_net::Error>,
}

impl From<gloo_net::Error> for TrackedGlooError {
    #[track_caller]
    fn from(err: gloo_net::Error) -> Self {
        Self {
            location: std::panic::Location::caller(),
            source: std::sync::Arc::new(err),
        }
    }
}
