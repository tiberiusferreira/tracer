use crate::{printable_local_date, printable_local_date_ms, API_SERVER_URL_NO_TRAILING_SLASH};
use api_structs::ui::trace_view::{SingleChunkTraceQuery, Span, TraceChunkId, TraceId};
use api_structs::Severity;
use leptos::ev::MouseEvent;
use leptos::logging::log;
use leptos::{
    component, create_action, create_local_resource, create_signal, view, CollectView, Fragment,
    IntoView, ReadSignal, Signal, SignalGet, SignalSet, WriteSignal,
};
use leptos_router::ParamsMap;
use std::collections::HashMap;
use std::ops::Deref;
use std::rc::Rc;

fn span_detail(trace_spans_r: Signal<Option<ApiTraceData>>) -> Fragment {
    let spans = trace_spans_r.get();
    let api_trace_data = match spans {
        None => {
            return view! { <><p style="color: white">{format!("Empty, crashed or still loading trace 😅. Check the network tab.")}</p></>};
        }
        Some(spans) if spans.spans.is_empty() => {
            return view! { <><p style="color: white">{format!("Empty trace 😅.")}</p></>};
        }
        Some(spans) => spans,
    };
    let spans = api_trace_data.spans;
    let start_timestamp_nanos = api_trace_data.chunk_id.start_timestamp;
    let el_count = spans
        .iter()
        .fold(0, |acc, curr| acc + curr.events.len() + 1);
    log!("{el_count} total span+events in trace");
    // let mut window_percentage = 100f64;
    // if el_count > 15_000 {
    //     window_percentage = 25f64;
    // }
    let root = spans
        .iter()
        .find(|d| d.parent_id.is_none())
        .cloned()
        .expect("No parent_span_id is null");

    let max_duration =
        api_trace_data.chunk_id.end_timestamp - api_trace_data.chunk_id.start_timestamp;

    let spans_by_parent_id: HashMap<i64, Vec<Span>> =
        spans.into_iter().fold(HashMap::new(), |mut acc, curr| {
            if let Some(parent_id) = curr.parent_id {
                acc.entry(parent_id).or_default().push(curr);
            }
            acc
        });
    let spans_by_parent_id = Rc::new(spans_by_parent_id);
    let max_duration_nanos = max_duration;
    // let mut html_span_and_children_summary = Vec::new();
    // let mut max_depth = 0;
    // create_summary_html_span_and_children_single_layer(
    //     start_timestamp_nanos,
    //     max_duration_nanos,
    //     &[root.clone()],
    //     Rc::clone(&spans_by_parent_id),
    //     0,
    //     &mut html_span_and_children_summary,
    //     &mut max_depth,
    // );
    // let container_ref = leptos::create_node_ref::<leptos::html::Div>();
    // let (read_x_offset_percentage, write_percentage) = create_signal(window_percentage / 2.);

    /*
          let percentage_0_to_100 = match curr.duration {
                None => 100.,
                Some(duration) => {
                    // clamp in case the span goes on for longer than the current window we are displaying
                    ((100 * duration) as f64 / max_duration as f64).min(100.)
                }
            };
    */
    let html_span_and_children = move || {
        // let percentage = read_x_offset_percentage.get();
        // let new_root_duration = ((max_duration_nanos as f64) * (window_percentage / 100.)) as u64;
        // let start_percentage = (percentage - window_percentage / 2.).max(0.);
        // let _end_percentage = (percentage + window_percentage / 2.).min(100.);
        // let new_root_start_offset =
        //     ((max_duration_nanos as f64) * (start_percentage / 100.)) as u64;
        // let new_root_start_micros = root_start_time_unix_nanos + new_root_start_offset;
        let mut html_span_and_children_fragments = Vec::with_capacity(0);
        create_html_span_and_children(
            start_timestamp_nanos,
            max_duration_nanos,
            &root,
            Rc::clone(&spans_by_parent_id),
            0,
            &mut html_span_and_children_fragments,
        );
        html_span_and_children_fragments
    };

    // let click_handler = move |ev: MouseEvent| {
    //     if window_percentage == 100. {
    //         return;
    //     }
    //     let click_x = ev.client_x();
    //     let container = container_ref.get().expect("container to already exist");
    //     let dom_rect = web_sys::Element::from(container.deref().clone()).get_bounding_client_rect();
    //     let container_start_x = dom_rect.x();
    //     let container_width = dom_rect.width();
    //     let click_x_offset_to_container = click_x as f64 - container_start_x;
    //     let x_offset_percentage =
    //         100. * (click_x_offset_to_container as f64 / container_width as f64);
    //     write_percentage.set(x_offset_percentage);
    // };
    // let height = max_depth * 20 + 15 + 16; // 16 is my "padding", 8 top, 8 bottom
    // let shadows = move || {
    //     let x_offset_percentage = read_x_offset_percentage.get();
    //     let shadow_left_end = (x_offset_percentage - window_percentage / 2.).max(0.);
    //     let shadow_right_start = (x_offset_percentage + window_percentage / 2.).min(100.);
    //     let shadow_right_width = 100. - shadow_right_start;
    //     view! {
    //         <>
    //             <div style=format!("margin-left:0%;width: {shadow_left_end:.2}%;height: {height}px;position: absolute;background-color: rgba(0, 0, 0, 0.6);z-index: 1;")></div>
    //             <div style=format!("margin-left:{shadow_right_start:.2}%;width: {shadow_right_width:.2}%;height: {height}px;position: absolute;background-color: rgba(0, 0, 0, 0.6);z-index: 1;")></div>
    //         </>
    //     }
    // };
    view! {
        <>
            // <div _ref=container_ref on:click=click_handler style=format!("background-color: rgba(255,255,255,0.05); margin: 15px 0 15px 0; height: {height}px; position: relative")>
            //     {shadows}
            //     {html_span_and_children_summary}
            // </div>
            <div>{html_span_and_children}</div>
        </>
    }
}

#[derive(Clone)]
struct ApiTraceData {
    trace_id: TraceId,
    chunk_id: TraceChunkId,
    spans: Vec<Span>,
}
#[component]
pub fn TraceDetails(root_path: String) -> impl IntoView {
    let query_parameters = leptos_router::use_query_map().get();
    let service_id = query_parameters
        .get("service_id")
        .unwrap()
        .parse::<i64>()
        .unwrap();
    let trace_id = query_parameters
        .get("trace_id")
        .unwrap()
        .parse::<i64>()
        .unwrap();
    let start_timestamp = query_parameters
        .get("start_timestamp")
        .map(|e| e.parse::<u64>().unwrap());
    let end_timestamp = query_parameters
        .get("end_timestamp")
        .map(|e| e.parse::<u64>().unwrap());
    let trace_id = TraceId {
        service_id,
        trace_id,
    };
    let (trace_spans_r, trace_spans_w) = leptos::create_signal(Option::<ApiTraceData>::None);
    let (trace_chunk_list_r, trace_chunk_list_w) = leptos::create_signal(Option::<Vec<u64>>::None);
    let (current_trace_chunk_r, current_trace_chunk_w) =
        leptos::create_signal(Option::<TraceChunkId>::None);

    let get_trace_chunk = {
        let trace_id = trace_id.clone();
        create_local_resource(
            move || current_trace_chunk_r.get(),
            move |chunk_id: Option<TraceChunkId>| {
                let trace_id = trace_id.clone();
                async move {
                    if let Some(chunk_id) = chunk_id {
                        let query = SingleChunkTraceQuery {
                            trace_id: trace_id.clone(),
                            chunk_id,
                        };
                        get_single_trace(query, trace_spans_w.clone()).await;
                    }
                }
            },
        )
    };
    if let (Some(start_timestamp), Some(end_timestamp)) = (start_timestamp, end_timestamp) {
        let chunk = TraceChunkId {
            start_timestamp,
            end_timestamp,
        };
        current_trace_chunk_w.set(Some(chunk.clone()));
    }
    let _api_chunk_list_request_sender = {
        let trace_id = trace_id.clone();
        leptos::create_local_resource(move || trace_id.clone(), {
            move |trace_id| {
                get_single_trace_chunk_list(
                    trace_id,
                    trace_chunk_list_w,
                    current_trace_chunk_r,
                    current_trace_chunk_w,
                )
            }
        })
    };

    let html_chunk_list = move || {
        let list = trace_chunk_list_r.get();
        let current_chunk_id = current_trace_chunk_r.get();
        match list {
            None => {
                view! {
                    <>
                    {"Loading chunks!"}
                    </>
                }
            }
            Some(chunks) => {
                let chunks = chunks
                    .iter()
                    .zip(chunks.iter().skip(1))
                    .enumerate()
                    .map(|(idx, (start, end))| {
                        let is_current = current_chunk_id.as_ref().map(|ct| ct.start_timestamp==*start && ct.end_timestamp==*end).unwrap_or(false);
                        let style = if is_current {
                            "margin: 5px; color: white".to_string()
                        }else{
                            "margin: 5px".to_string()
                        };
                        let dates = format!("{} - {}", printable_local_date(*start), printable_local_date(*end));
                        view!{
                            <>
                                <a style={style} target="_self" href={format!("{}trace/?service_id={}&trace_id={}&start_timestamp={}&&end_timestamp={}", root_path.clone(), trace_id.service_id, trace_id.trace_id, start, end)}>{format!("{} - {dates}", idx+1)}</a>
                            <>
                        }
                    });
                chunks.collect_view().into()
            }
        }
    };
    let html_spans = move || span_detail(Signal::from(trace_spans_r));
    view! {
        <div class="main-grid">
            <div class="main">
                <div class="trace-chunk-list">
                    {html_chunk_list}
                </div>
                <div class="trace-details">
                    {html_spans}
                </div>
            </div>
            <div class="search-panel">
                <label class="search-panel__label">
                    "TODO (span/event details):"
                    <input
                        class="search-panel__input" type="text" required=true minlength="3" maxlength="20" size="20"
                    />
                </label>
            </div>
        </div>
    }
}

#[derive(PartialEq, Clone)]
struct TraceGridRow {
    trace_id: i64,
    has_errors: bool,
    service_name: String,
    top_level_span_name: String,
    duration_ms: i32,
    sample_log: String,
    sample_log_kv: Vec<(String, String)>,
    created_at_unix_ms: i64,
}

async fn get_single_trace_chunk_list(
    TraceId {
        service_id,
        trace_id,
    }: TraceId,
    w: WriteSignal<Option<Vec<u64>>>,
    current_chunk_r: ReadSignal<Option<TraceChunkId>>,
    current_chunk_w: WriteSignal<Option<TraceChunkId>>,
) {
    log!("Sending req");
    let chunks: Vec<u64> = gloo_net::http::Request::get(&format!(
        "{}/api/trace_chunk_list?service_id={service_id}&trace_id={trace_id}",
        API_SERVER_URL_NO_TRAILING_SLASH,
    ))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    log!("Got back");
    if current_chunk_r.get().is_none() {
        if chunks.len() == 1 {
            current_chunk_w.set(Some(TraceChunkId {
                start_timestamp: chunks[0],
                end_timestamp: chunks[0],
            }))
        } else {
            let mut last_two = chunks.iter().rev().take(2).rev();
            let before_last = last_two.next().unwrap();
            let last = last_two.next().unwrap();
            current_chunk_w.set(Some(TraceChunkId {
                start_timestamp: *before_last,
                end_timestamp: *last,
            }))
        }
    }
    w.set(Some(chunks));
}
async fn get_single_trace(
    SingleChunkTraceQuery {
        trace_id: TraceId {
            service_id,
            trace_id,
        },
        chunk_id: TraceChunkId {
            start_timestamp,
            end_timestamp,
        },
    }: SingleChunkTraceQuery,
    w: WriteSignal<Option<ApiTraceData>>,
) {
    log!("Sending req");
    let spans: Vec<Span> = gloo_net::http::Request::get(&format!(
        "{}/api/trace?service_id={service_id}&trace_id={trace_id}&start_timestamp={start_timestamp}&end_timestamp={end_timestamp}",
        API_SERVER_URL_NO_TRAILING_SLASH,
    ))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    log!("Got back");
    let api_trace_data = ApiTraceData {
        trace_id: TraceId {
            service_id,
            trace_id,
        },
        chunk_id: TraceChunkId {
            start_timestamp,
            end_timestamp,
        },
        spans,
    };
    w.set(Some(api_trace_data));
}

fn create_html_span_and_children(
    start_time_unix_nanos: u64,
    max_duration_nanos: u64,
    span: &Span,
    spans_by_parent_id: Rc<HashMap<i64, Vec<Span>>>,
    depth: i32,
    html_span_and_children_fragments: &mut Vec<Fragment>,
) {
    let empty = vec![];
    let mut children: Vec<Span> = spans_by_parent_id.get(&span.id).unwrap_or(&empty).clone();
    children.sort_by_key(|k| k.timestamp);
    if let Some(e) = create_html_span(start_time_unix_nanos, max_duration_nanos, span, depth) {
        html_span_and_children_fragments.push(e);
    }
    for c in &children {
        create_html_span_and_children(
            start_time_unix_nanos,
            max_duration_nanos,
            c,
            Rc::clone(&spans_by_parent_id),
            depth + 1,
            &mut *html_span_and_children_fragments,
        );
    }
}

fn create_summary_html_span_and_children_single_layer(
    start_time_unix_micros: u64,
    max_duration: u64,
    spans: &[Span],
    spans_by_parent_id: Rc<HashMap<i64, Vec<Span>>>,
    curr_depth: i32,
    html_span_and_children_summary: &mut Vec<Fragment>,
    max_depth: &mut i32,
) {
    // if curr_depth > *max_depth {
    //     *max_depth = curr_depth;
    // }
    // let empty = vec![];
    // let mut next_layer_spans = spans.iter().fold(Vec::new(), |mut acc, curr| {
    //     acc.extend_from_slice(&spans_by_parent_id.get(&curr.id).unwrap_or(&empty));
    //     acc
    // });
    // next_layer_spans.sort_by_key(|k| k.timestamp);
    //
    // // group very small children into "single" element
    // let spans = spans
    //     .to_vec()
    //     .into_iter()
    //     .fold(Vec::new(), |mut acc: Vec<Span>, curr| {
    //         let percentage_0_to_100 = match curr.duration {
    //             None => 100.,
    //             Some(duration) => {
    //                 // clamp in case the span goes on for longer than the current window we are displaying
    //                 ((100 * duration) as f64 / max_duration as f64).min(100.)
    //             }
    //         };
    //
    //         // if percentage_0_to_100 < 0.2 {
    //         //     if let Some(last) = acc.last_mut() {
    //         //         let last_plus_curr_combined_duration = last.duration + curr.duration;
    //         //         let combined_percentage_0_to_100 =
    //         //             (100 * last_plus_curr_combined_duration) as f64 / max_duration as f64;
    //         //         let last_end_time = last.timestamp + last.duration;
    //         //         let gap_micros = curr.timestamp.saturating_sub(last_end_time);
    //         //         if combined_percentage_0_to_100 < 0.2 && gap_micros < 1000 {
    //         //             last.duration += curr.duration + gap_micros;
    //         //             return acc;
    //         //         }
    //         //     }
    //         // }
    //         acc.push(curr);
    //         acc
    //     });
    // let mut last_end: u64 = 0;
    // for s in spans {
    //     // dont let them overlap when multiple spans happened at ~ the same time
    //     let overlap = last_end.saturating_sub(s.timestamp);
    //     if s.duration >= overlap * 2 {
    //         html_span_and_children_summary.push(create_span_summary_html(
    //             start_time_unix_micros,
    //             max_duration,
    //             s.timestamp,
    //             s.duration,
    //             curr_depth,
    //             &s.name,
    //         ));
    //         last_end = s.timestamp + s.duration;
    //     }
    // }
    // if !next_layer_spans.is_empty() {
    //     create_summary_html_span_and_children_single_layer(
    //         start_time_unix_micros,
    //         max_duration,
    //         &next_layer_spans,
    //         Rc::clone(&spans_by_parent_id),
    //         curr_depth + 1,
    //         &mut *html_span_and_children_summary,
    //         max_depth,
    //     );
    // }
    unimplemented!()
}

fn create_span_summary_html(
    root_start_time_unix_micros: u64,
    root_duration_micros: u64,
    start_time_unix_micros: u64,
    duration_micros: u64,
    depth: i32,
    span_name: &str,
) -> Fragment {
    let start_offset_micros;
    let start_offset_percentage;
    let duration_percentage: f64;
    start_offset_micros = start_time_unix_micros - root_start_time_unix_micros;
    start_offset_percentage = (100 * start_offset_micros) as f64 / root_duration_micros as f64;
    duration_percentage = ((100 * duration_micros) as f64 / root_duration_micros as f64)
        .max(0.2)
        .min(100f64 - start_offset_percentage);
    let mut depth_to_color: HashMap<i32, String> = HashMap::new();
    depth_to_color.insert(0, "white".to_string());
    depth_to_color.insert(1, "red".to_string());
    depth_to_color.insert(2, "green".to_string());
    depth_to_color.insert(3, "blue".to_string());
    depth_to_color.insert(4, "purple".to_string());
    depth_to_color.insert(5, "brown".to_string());
    depth_to_color.insert(6, "darkred".to_string());
    depth_to_color.insert(7, "forestgreen".to_string());
    let margin_top = 8 + depth * 20; // 8 my "padding"
    let span_style = format!(
        "position: absolute; margin-top: {margin_top}px; height: 15px; background-color: {}; border-radius: 8px",
        depth_to_color.get(&(depth % 8)).unwrap()
    );
    let paragraph = if duration_percentage >= (span_name.len() as f64 / 2.) {
        Some(view! {
            <p style="margin: 0; font-size: x-small; text-align: center">{span_name.to_string()}</p>
        })
    } else {
        None
    };
    let span_html = view! {
        <>
        <div style={format!("margin-left: {start_offset_percentage}%; width: {duration_percentage}%; {}", span_style)}>
            {paragraph}
        </div>
        </>
    };
    span_html
}

fn create_html_span(
    start_timestamp_nanos: u64,
    max_duration: u64,
    span: &Span,
    depth: i32,
) -> Option<Fragment> {
    let span_start = span.timestamp;
    let span_duration = span.duration.map(|d| d.max(1)); // make it not 0
    let end_timestamp = start_timestamp_nanos + max_duration;
    // span may start before the start_timestamp_nanos
    let start_offset_nanos = (span_start.saturating_sub(start_timestamp_nanos));
    log!("span_start={span_start}");
    log!("start_timestamp_nanos={start_timestamp_nanos}");
    log!("start_offset_nanos={start_offset_nanos}");
    log!("max_duration={max_duration}");
    let start_offset_percentage: f64 = (100 * start_offset_nanos) as f64 / max_duration as f64;
    let max_duration_percentage = 100. - start_offset_percentage;
    let duration_percentage: f64 = match span_duration {
        None => max_duration_percentage,
        Some(duration) => ((100 * duration) as f64 / max_duration as f64)
            .max(0.2)
            .min(max_duration_percentage),
    };
    let mut depth_to_color: HashMap<i32, String> = HashMap::new();
    depth_to_color.insert(0, "white".to_string());
    depth_to_color.insert(1, "red".to_string());
    depth_to_color.insert(2, "green".to_string());
    depth_to_color.insert(3, "blue".to_string());
    depth_to_color.insert(4, "purple".to_string());
    depth_to_color.insert(5, "brown".to_string());
    depth_to_color.insert(6, "darkred".to_string());
    depth_to_color.insert(7, "forestgreen".to_string());
    let span_style = format!(
        "margin-top: 0; height: 10px; background-color: {}; border-radius: 8px",
        depth_to_color.get(&(depth % 8)).unwrap()
    );
    let mut ordered_events = span.events.clone();
    ordered_events.sort_by_key(|e| e.timestamp);
    let events: Vec<_> = ordered_events
        .iter()
        .map(|e| {
            let color = match e.severity{
                Severity::Warn => {
                    "yellow"
                }
                Severity::Error => {
                    "red"
                }
                _ => {
                    "white"
                }
            };
            let event_k_v: Vec<String> = e.key_values
                .iter()
                .map(|(k, v)| format!("{k} => {v}"))
                .collect();
            let event_k_v = if !event_k_v.is_empty() {
                format!(" - {}", event_k_v.join(", "))
            } else {
                "".to_string()
            };
            let event_date = printable_local_date_ms(e.timestamp);
            let event_msg = format!("{} - {}{}", event_date, e.message.as_ref().unwrap_or(&"null".to_string()), event_k_v);
            // event offset % calculation
            let event_nanos_after_trace_start = e.timestamp
                .checked_sub(start_timestamp_nanos).unwrap();
            let event_percentage_into_trace_duration =
                100.* event_nanos_after_trace_start as f64/ max_duration as f64;
            // don't got over 99.6 because we need to display the character itself too
            let event_percentage_into_trace_duration = event_percentage_into_trace_duration.min(99.6);
            view! {
                <div style="width: 100%; background-color: rgba(255,255,255,0.05)">
                    <p style={format!("margin-left: {event_percentage_into_trace_duration}%")} class="trace-details__event-timestamp">{"|"}</p>
                    <p class="trace-details__event" style={format!("color: {color}")}>{event_msg}</p>
                </div>
            }
        })
        .collect();
    let span_k_v: Vec<String> = span
        .key_values
        .iter()
        .map(|(k, v)| format!("{k} => {v}"))
        .collect();
    let span_k_v = if !span_k_v.is_empty() {
        format!(" - {}", span_k_v.join(", "))
    } else {
        "".to_string()
    };
    let span_with_code_namespace = span.name.to_string();
    let span_duration_ms_string = match span.duration {
        None => "still running".to_string(),
        Some(duration) => {
            format!("{}ms", duration / 1000_000)
        }
    };

    let span_html = view! {
        <>
        <p class="trace-details__span-name">{format!("{} - {span_duration_ms_string}{span_k_v}", span_with_code_namespace)}</p>
        <div style={format!("margin-left: {start_offset_percentage}%; width: {duration_percentage}%; {}", span_style)}></div>
            {events}
        </>
    };
    Some(span_html)
}
