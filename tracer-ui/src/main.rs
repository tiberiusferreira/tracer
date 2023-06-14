use leptos::*;

mod grid;
use grid::TraceGrid;
mod details;
mod summary;
use details::TraceDetails;
use leptos_router::*;
use summary::TracesSummary;
const API_SERVER_URL_NO_TRAILING_SLASH: &str = env!("API_SERVER_URL_NO_TRAILING_SLASH");

fn main() {
    _ = console_log::init();
    console_error_panic_hook::set_once();
    mount_to_body(|cx| view! { cx,  <App/> });
    log!("Loaded up!");
}

#[component]
pub fn App(cx: Scope) -> impl IntoView {
    let root_path = "/".to_string();
    view! { cx,
        <>
            <header>
                <nav class="navigation">
                    <div class="navigation__button"></div>
                    <a class="navigation__button" href={&root_path}>"Home"</a>
                    <a class="navigation__button" href=format!("{}summary", root_path)>"Summary"</a>
                </nav>
            </header>
                <Router>
                    <Routes>
                        <Route
                              path=root_path.clone()
                              view={
                                    let root_path= root_path.to_string();
                                    move |cx|{
                                        view! {
                                            cx,  <TraceGrid root_path=root_path.clone()/>
                                        }
                                    }
                                }
                            />
                        <Route
                              path=format!("{}trace", root_path)
                              view=move |cx| view! {
                                    cx,
                                    <TraceDetails/>
                                }
                            />
                        <Route
                              path=format!("{}summary", root_path)
                              view={
                                let root_path= root_path.to_string();
                                move |cx| view! {
                                    cx,
                                    <TracesSummary root_path=root_path.clone()/>
                                }
                              }
                            />
                    </Routes>
                </Router>
        </>
    }
}
