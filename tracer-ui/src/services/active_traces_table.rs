use crate::secs_since;
use api_structs::time_conversion::now_nanos_u64;
use api_structs::ui::service_health::{ServiceData, TraceHeader};
use leptos::html::Div;
use leptos::ReadSignal;
use leptos::{view, IntoView, SignalGet};
pub fn active_traces_table_html(
    active_trace_graph_click_event_on_timestamp_r: ReadSignal<Option<u64>>,
    service: ServiceData,
    root_path: String,
) -> leptos::HtmlElement<Div> {
    let view = move || {
        let timestamp: Option<u64> = match active_trace_graph_click_event_on_timestamp_r.get() {
            None => {
                let mut timestamp = None;
                for i in &service.instances {
                    if let Some(data_point) = i.time_data_points.last() {
                        timestamp = Some(data_point.timestamp);
                        break;
                    }
                }
                timestamp
            }
            Some(timestamp) => Some(timestamp),
        };
        let timestamp = timestamp.unwrap_or(now_nanos_u64());
        let window_secs = 3;
        let window_nanos = window_secs * 1000_000_000;
        #[derive(Clone)]
        struct TraceHeaderWithInstance {
            trace_header: TraceHeader,
            instance_id: i64,
        }
        let mut active_traces = vec![];
        let mut finished_traces = vec![];
        for i in &service.instances {
            for d in &i.time_data_points {
                if (timestamp - window_nanos) < d.timestamp
                    && d.timestamp < (timestamp + window_nanos)
                {
                    active_traces.extend_from_slice(
                        &d.active_traces
                            .iter()
                            .map(|trace_header| TraceHeaderWithInstance {
                                trace_header: TraceHeader {
                                    trace_id: trace_header.trace_id,
                                    trace_name: trace_header.trace_name.clone(),
                                    trace_timestamp: trace_header.trace_timestamp,
                                    duration: trace_header.duration,
                                },
                                instance_id: i.id,
                            })
                            .collect::<Vec<TraceHeaderWithInstance>>(),
                    );
                    finished_traces.extend_from_slice(
                        &d.finished_traces
                            .iter()
                            .map(|trace_header| TraceHeaderWithInstance {
                                trace_header: TraceHeader {
                                    trace_id: trace_header.trace_id,
                                    trace_name: trace_header.trace_name.clone(),
                                    trace_timestamp: trace_header.trace_timestamp,
                                    duration: trace_header.duration,
                                },
                                instance_id: i.id,
                            })
                            .collect::<Vec<TraceHeaderWithInstance>>(),
                    );
                }
            }
        }
        let mut active_trace_els = vec![];
        for active in active_traces {
            active_trace_els.push(view! {
                <tr class={"row-container"}>
                    <td class="trace-table__cell">{active.trace_header.trace_name}</td>
                    <td class="trace-table__cell">{active.instance_id}</td>
                    <td class="trace-table__cell">{secs_since(active.trace_header.trace_timestamp)}</td>
                    <td class="trace-table__cell">{active.trace_header.duration.map(|e| (e/1000_000).to_string()).unwrap_or(format!("{} seconds - Still Running", secs_since(active.trace_header.trace_timestamp)))}</td>
                    <td class="trace-table__cell">
                        <a href={format!("{}trace/?service_id={}&trace_id={}&start_timestamp={}", root_path, active.instance_id, active.trace_header.trace_id, active.trace_header.trace_timestamp)}>{"➔"}</a>
                    </td>
                </tr>
            });
        }

        let mut finished_trace_els = vec![];
        for finished in finished_traces {
            finished_trace_els.push(view! {
                <tr class={"row-container"}>
                    <td class="trace-table__cell">{finished.trace_header.trace_name}</td>
                    <td class="trace-table__cell">{finished.instance_id}</td>
                    <td class="trace-table__cell">{secs_since(finished.trace_header.trace_timestamp)}</td>
                    <td class="trace-table__cell">{finished.trace_header.duration.map(|e| (e/1000_000).to_string()).unwrap_or_default()}</td>
                    <td class="trace-table__cell">
                        <a href={format!("{}trace/?service_id={}&trace_id={}&start_timestamp={}", root_path, finished.instance_id, finished.trace_header.trace_id, finished.trace_header.trace_timestamp)}>{"➔"}</a>
                    </td>
                </tr>
            });
        }

        view! {
            <>
             <p style="text-align: center">{format!("Active Traces {:?} sec ago (+- 3s)   ", secs_since(timestamp))}
             </p>
                    <table class="trace-table">
                        <tr class="row-container">
                            <th style="text-align: center" colspan="5" class="trace-table__cell">
                                <a>"Active"</a>
                            </th>
                        </tr>
                        <tr class="row-container">
                            <th class="trace-table__cell">
                                <a>"Trace Name"</a>
                            </th>
                            <th class="trace-table__cell">
                                <a>"Instance Id"</a>
                            </th>
                            <th class="trace-table__cell">
                                <a>"Secs Ago"</a>
                            </th>
                            <th class="trace-table__cell">
                                <a>{"Duration (ms)"}</a>
                            </th>
                            <th class="trace-table__cell">
                                <a>{"➔"}</a>
                            </th>
                        </tr>
                        {active_trace_els}
                        <tr class="row-container">
                            <th style="text-align: center" colspan="5" class="trace-table__cell">
                                <a>"Finished"</a>
                            </th>
                        </tr>
                        {finished_trace_els}
                    </table>
            </>
        }
    };
    view! {
        <div>
          {view}
        </div>
    }
}
