use crate::datetime::secs_since;
use crate::orphan_events::orphan_events_to_html;
use crate::{PAGE_ROOT_URL, TRACE_CHUNK_PATH};
use api_structs::time_conversion::now_nanos_u64;
use api_structs::ui::service::{ServiceOverview, TraceHeader};
use api_structs::InstanceId;
use leptos::html::Div;
use leptos::ReadSignal;
use leptos::{view, SignalGet};

pub fn get_html(
    active_trace_graph_click_event_on_timestamp_r: ReadSignal<Option<u64>>,
    service: ServiceOverview,
) -> leptos::HtmlElement<Div> {
    let view = move || {
        let timestamp: Option<u64> = match active_trace_graph_click_event_on_timestamp_r.get() {
            None => service.service_data_over_time.last().map(|e| e.timestamp),
            Some(timestamp) => Some(timestamp),
        };
        let timestamp = timestamp.unwrap_or(now_nanos_u64());
        let window_secs = 3;
        let window_nanos = window_secs * 1000_000_000;
        #[derive(Clone)]
        struct TraceHeaderWithInstance {
            instance_id: InstanceId,
            trace_header: TraceHeader,
        }
        let mut active_traces = vec![];
        let mut finished_traces = vec![];
        let mut orphan_events = vec![];
        for d in &service.service_data_over_time {
            if (timestamp - window_nanos) < d.timestamp && d.timestamp < (timestamp + window_nanos)
            {
                orphan_events.extend_from_slice(&d.orphan_events);
                active_traces.extend_from_slice(
                    &d.active_traces
                        .iter()
                        .map(|trace_header| TraceHeaderWithInstance {
                            trace_header: TraceHeader {
                                trace_id: trace_header.trace_id,
                                trace_name: trace_header.trace_name.clone(),
                                trace_timestamp: trace_header.trace_timestamp,
                                new_warnings: trace_header.new_warnings,
                                new_errors: trace_header.new_errors,
                                fragment_bytes: trace_header.fragment_bytes,
                                duration: trace_header.duration,
                            },
                            instance_id: InstanceId {
                                service_id: service.service_id.clone(),
                                instance_id: d.instance_id,
                            },
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
                                new_warnings: trace_header.new_warnings,
                                new_errors: trace_header.new_errors,
                                fragment_bytes: trace_header.fragment_bytes,
                                duration: trace_header.duration,
                            },
                            instance_id: InstanceId {
                                service_id: service.service_id.clone(),
                                instance_id: d.instance_id,
                            },
                        })
                        .collect::<Vec<TraceHeaderWithInstance>>(),
                );
            }
        }
        let mut active_trace_els = vec![];
        for active in active_traces {
            let row_container_class = if active.trace_header.new_errors {
                "row-container row-container__error".to_string()
            } else if active.trace_header.new_warnings {
                "row-container row-container__warning".to_string()
            } else {
                "row-container".to_string()
            };
            active_trace_els.push(view! {
                <tr class={row_container_class}>
                    <td class="trace-table__cell">{active.trace_header.trace_name}</td>
                    <td class="trace-table__cell">{active.instance_id.instance_id}</td>
                    <td class="trace-table__cell">{secs_since(active.trace_header.trace_timestamp)}</td>
                    <td class="trace-table__cell">{format!("{:.2}", active.trace_header.fragment_bytes as f32/100.)}</td>
                    <td class="trace-table__cell">{active.trace_header.duration.map(|e| (e/1000_000).to_string()).unwrap_or(format!("{} seconds - Still Running", secs_since(active.trace_header.trace_timestamp)))}</td>
                    <td class="trace-table__cell">
                        <a href={format!("{}{TRACE_CHUNK_PATH}/?env={}&service_name={}&instance_id={}&trace_id={}&start_timestamp={}", PAGE_ROOT_URL, active.instance_id.service_id.env, active.instance_id.service_id.name, active.instance_id.instance_id, active.trace_header.trace_id, active.trace_header.trace_timestamp)}>{"➔"}</a>
                    </td>
                </tr>
            });
        }

        let mut finished_trace_els = vec![];
        for finished in finished_traces {
            let row_container_class = if finished.trace_header.new_errors {
                "row-container row-container__error".to_string()
            } else if finished.trace_header.new_warnings {
                "row-container row-container__warning".to_string()
            } else {
                "row-container".to_string()
            };
            finished_trace_els.push(view! {
                <tr class={row_container_class}>
                    <td class="trace-table__cell">{finished.trace_header.trace_name}</td>
                    <td class="trace-table__cell">{finished.instance_id.instance_id}</td>
                    <td class="trace-table__cell">{secs_since(finished.trace_header.trace_timestamp)}</td>
                    <td class="trace-table__cell">{format!("{:.2}", finished.trace_header.fragment_bytes as f32/100.)}</td>
                    <td class="trace-table__cell">{finished.trace_header.duration.map(|e| (e/1000_000).to_string()).unwrap_or_default()}</td>
                    <td class="trace-table__cell">
                        <a href={format!("{}{TRACE_CHUNK_PATH}/?env={}&service_name={}&instance_id={}&trace_id={}&start_timestamp={}", PAGE_ROOT_URL, finished.instance_id.service_id.env, finished.instance_id.service_id.name, finished.instance_id.instance_id, finished.trace_header.trace_id, finished.trace_header.trace_timestamp)}>{"➔"}</a>
                    </td>
                </tr>
            });
        }
        let orphan_events_html = orphan_events_to_html(&orphan_events, true);

        view! {
            <>
                <div style="max-height: 450px; overflow: auto; padding: 20px; color: white">
                    <p style="text-align: center">{format!("Active Traces {:?} sec ago (+- 3s)   ", secs_since(timestamp))}</p>
                    <table class="trace-table">
                        <tr class="row-container">
                            <th style="text-align: center" colspan="6" class="trace-table__cell">
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
                                <a>"KB"</a>
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
                            <th style="text-align: center" colspan="6" class="trace-table__cell">
                                <a>"Finished"</a>
                            </th>
                        </tr>
                        {finished_trace_els}
                    </table>
                </div>
                {orphan_events_html}

            </>
        }
    };
    view! {
        <div>
          {view}
        </div>
    }
}
