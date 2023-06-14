use crate::API_SERVER_URL_NO_TRAILING_SLASH;
use api_structs::{Summary, SummaryRequest};
use leptos::{
    component, log, view, HtmlElement, IntoView, Scope, SignalGet, SignalSet, WriteSignal,
};

#[component]
pub fn TracesSummary(cx: Scope, root_path: String) -> impl IntoView {
    let (trace_spans_r, trace_spans_w) = leptos::create_signal(cx, Vec::new());
    let _api_request_sender =
        leptos::create_local_resource(cx, move || (), move |_| get_summary(trace_spans_w));

    let html_headers = [
        view! {cx,
            <th class="trace-table__cell">
                <a>"Service Name"</a>
            </th>
        },
        view! {cx,
            <th class="trace-table__cell">
                <a>"Top Level Span"</a>
            </th>
        },
        view! {cx,
            <th class="trace-table__cell" style="cursor: pointer" >
                <a>"Total Traces"</a>
            </th>
        },
        view! {cx,
            <th class="trace-table__cell">
                <a>"Traces with Error"</a>
            </th>
        },
        view! {cx,
            <th class="trace-table__cell">
                <a>"Longest Trace (ms)"</a>
            </th>
        },
        view! {cx,
            <th class="trace-table__cell">
                <a>"➔"</a>
            </th>
        },
    ]
    .to_vec();
    let html_rows = move |rows: Vec<Summary>| {
        let res: Vec<HtmlElement<_>> = rows.into_iter().map(|r|{
                view! {
                cx,
                <tr class="row_container_class">
                        <td class="trace-table__cell">{r.service_name.clone()}</td>
                        <td class="trace-table__cell">{r.top_level_span_name.clone()}</td>
                        <td class="trace-table__cell">{r.total_traces}</td>
                        <td class="trace-table__cell">{r.total_traces_with_error}</td>
                        <td class="trace-table__cell">{r.longest_trace_duration/1000_000}</td>
                        <td class="trace-table__cell">
                    <a href={format!("{}trace/?trace_id={}", root_path, r.longest_trace_id)}>{"➔"}</a>
                        </td>
                </tr>
                }
            }).collect();
        res
    };

    view! {cx,
        <div class="main-grid">
            <div class="main">
                <table class="trace-table">
                    <tr class="row-container">
                            {html_headers}
                    </tr>
                    {move || html_rows(trace_spans_r.get())}
                </table>
            </div>
            <div class="search-panel">
                <label class="search-panel__label">
                    "Containing:"
                    <input
                        class="search-panel__input" type="text" required=true minlength="3" maxlength="20" size="20"
                    />
                </label>
            </div>
        </div>
    }
}

async fn get_summary(w: WriteSignal<Vec<Summary>>) {
    log!("Sending req");
    let traces: Vec<Summary> =
        gloo_net::http::Request::post(&format!("{}/api/summary", API_SERVER_URL_NO_TRAILING_SLASH))
            .json(&SummaryRequest {
                from_date_unix_micros: 100,
                to_date_unix_micros: 99999,
            })
            .unwrap()
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
    log!("Got summary back");
    w.set(traces);
}
