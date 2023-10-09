use crate::API_SERVER_URL_NO_TRAILING_SLASH;
use api_structs::{Severity, Span};
use leptos::ev::MouseEvent;
use leptos::logging::log;
use leptos::{
    component, create_signal, view, Fragment, IntoView, Signal, SignalGet, SignalSet, WriteSignal,
};
use leptos_router::ParamsMap;
use std::collections::HashMap;
use std::ops::Deref;
use std::rc::Rc;

fn span_detail(trace_spans_r: Signal<Vec<Span>>) -> Fragment {
    let spans = trace_spans_r.get();
    if spans.is_empty() {
        return view! { <><p style="color: white">{format!("Empty, crashed or still loading trace 😅. Check the network tab.")}</p></>};
    }
    let el_count = spans
        .iter()
        .fold(0, |acc, curr| acc + curr.events.len() + 1);
    log!("{el_count} total span+events in trace");
    let mut window_percentage = 100f64;
    if el_count > 15_000 {
        window_percentage = 25f64;
    }
    let root = spans
        .iter()
        .find(|d| d.parent_id.is_none())
        .cloned()
        .expect("No parent_span_id is null");
    let spans_by_parent_id: HashMap<u64, Vec<Span>> =
        spans.into_iter().fold(HashMap::new(), |mut acc, curr| {
            if let Some(parent_id) = curr.parent_id {
                acc.entry(parent_id).or_default().push(curr);
            }
            acc
        });
    let spans_by_parent_id = Rc::new(spans_by_parent_id);
    let root_start_time_unix_micros = root.timestamp;
    let root_duration_micros = root.duration;
    let mut html_span_and_children_summary = Vec::new();
    let mut max_depth = 0;
    create_summary_html_span_and_children_single_layer(
        root_start_time_unix_micros,
        root_duration_micros,
        &[root.clone()],
        Rc::clone(&spans_by_parent_id),
        0,
        &mut html_span_and_children_summary,
        &mut max_depth,
    );
    let container_ref = leptos::create_node_ref::<leptos::html::Div>();
    let (read_x_offset_percentage, write_percentage) = create_signal(window_percentage / 2.);

    let html_span_and_children = move || {
        let percentage = read_x_offset_percentage.get();
        let new_root_duration = ((root_duration_micros as f64) * (window_percentage / 100.)) as u64;
        let start_percentage = (percentage - window_percentage / 2.).max(0.);
        let _end_percentage = (percentage + window_percentage / 2.).min(100.);
        let mut html_span_and_children_fragments = Vec::with_capacity(0);
        let new_root_start_offset =
            ((root_duration_micros as f64) * (start_percentage / 100.)) as u64;
        let new_root_start_micros = root_start_time_unix_micros + new_root_start_offset;
        create_html_span_and_children(
            new_root_start_micros,
            new_root_duration,
            &root,
            Rc::clone(&spans_by_parent_id),
            0,
            &mut html_span_and_children_fragments,
        );
        html_span_and_children_fragments
    };

    let click_handler = move |ev: MouseEvent| {
        if window_percentage == 100. {
            return;
        }
        let click_x = ev.client_x();
        let container = container_ref.get().expect("container to already exist");
        let dom_rect = web_sys::Element::from(container.deref().clone()).get_bounding_client_rect();
        let container_start_x = dom_rect.x();
        let container_width = dom_rect.width();
        let click_x_offset_to_container = click_x as f64 - container_start_x;
        let x_offset_percentage =
            100. * (click_x_offset_to_container as f64 / container_width as f64);
        write_percentage.set(x_offset_percentage);
    };
    let height = max_depth * 20 + 15 + 16; // 16 is my "padding", 8 top, 8 bottom
    let shadows = move || {
        let x_offset_percentage = read_x_offset_percentage.get();
        let shadow_left_end = (x_offset_percentage - window_percentage / 2.).max(0.);
        let shadow_right_start = (x_offset_percentage + window_percentage / 2.).min(100.);
        let shadow_right_width = 100. - shadow_right_start;
        view! {
            <>
                <div style=format!("margin-left:0%;width: {shadow_left_end:.2}%;height: {height}px;position: absolute;background-color: rgba(0, 0, 0, 0.6);z-index: 1;")></div>
                <div style=format!("margin-left:{shadow_right_start:.2}%;width: {shadow_right_width:.2}%;height: {height}px;position: absolute;background-color: rgba(0, 0, 0, 0.6);z-index: 1;")></div>
            </>
        }
    };
    view! {
        <>
            <div _ref=container_ref on:click=click_handler style=format!("background-color: rgba(255,255,255,0.05); margin: 15px 0 15px 0; height: {height}px; position: relative")>
                {shadows}
                {html_span_and_children_summary}
            </div>
            <div>{html_span_and_children}</div>
        </>
    }
}

#[component]
pub fn TraceDetails() -> impl IntoView {
    let query_parameters = leptos_router::use_query_map().get();
    let (trace_spans_r, trace_spans_w) = leptos::create_signal(Vec::new());
    let _api_request_sender = leptos::create_local_resource(move || query_parameters.clone(), {
        move |qp| get_single_trace(qp, trace_spans_w)
    });
    let html_spans = move || span_detail(Signal::from(trace_spans_r));
    view! {
        <div class="main-grid">
            <div class="main">
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

async fn get_single_trace(id: ParamsMap, w: WriteSignal<Vec<Span>>) {
    log!("Sending req");
    let traces: Vec<Span> = gloo_net::http::Request::get(&format!(
        "{}/api/trace{}",
        API_SERVER_URL_NO_TRAILING_SLASH,
        id.to_query_string()
    ))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    log!("Got back");
    w.set(traces);
}

fn create_html_span_and_children(
    root_start_time_unix_micros: u64,
    root_duration_micros: u64,
    span: &Span,
    spans_by_parent_id: Rc<HashMap<u64, Vec<Span>>>,
    depth: i32,
    html_span_and_children_fragments: &mut Vec<Fragment>,
) {
    let empty = vec![];
    let mut children: Vec<Span> = spans_by_parent_id.get(&span.id).unwrap_or(&empty).clone();
    children.sort_by_key(|k| k.timestamp);
    if let Some(e) = create_html_span(
        root_start_time_unix_micros,
        root_duration_micros,
        span,
        depth,
    ) {
        html_span_and_children_fragments.push(e);
    }
    for c in &children {
        create_html_span_and_children(
            root_start_time_unix_micros,
            root_duration_micros,
            c,
            Rc::clone(&spans_by_parent_id),
            depth + 1,
            &mut *html_span_and_children_fragments,
        );
    }
}

fn create_summary_html_span_and_children_single_layer(
    root_start_time_unix_micros: u64,
    root_duration_micros: u64,
    spans: &[Span],
    spans_by_parent_id: Rc<HashMap<u64, Vec<Span>>>,
    curr_depth: i32,
    html_span_and_children_summary: &mut Vec<Fragment>,
    max_depth: &mut i32,
) {
    if curr_depth > *max_depth {
        *max_depth = curr_depth;
    }
    let empty = vec![];
    let mut next_layer_spans = spans.iter().fold(Vec::new(), |mut acc, curr| {
        acc.extend_from_slice(&spans_by_parent_id.get(&curr.id).unwrap_or(&empty));
        acc
    });
    next_layer_spans.sort_by_key(|k| k.timestamp);

    // group very small children into "single" element
    let spans = spans
        .to_vec()
        .into_iter()
        .fold(Vec::new(), |mut acc: Vec<Span>, curr| {
            let percentage_0_to_100 = (100 * curr.duration) as f64 / root_duration_micros as f64;
            if percentage_0_to_100 < 0.2 {
                if let Some(last) = acc.last_mut() {
                    let last_plus_curr_combined_duration = last.duration + curr.duration;
                    let combined_percentage_0_to_100 = (100 * last_plus_curr_combined_duration)
                        as f64
                        / root_duration_micros as f64;
                    let last_end_time = last.timestamp + last.duration;
                    let gap_micros = curr.timestamp.saturating_sub(last_end_time);
                    if combined_percentage_0_to_100 < 0.2 && gap_micros < 1000 {
                        last.duration += curr.duration + gap_micros;
                        return acc;
                    }
                }
            }
            acc.push(curr);
            acc
        });
    let mut last_end: u64 = 0;
    for s in spans {
        // dont let them overlap when multiple spans happened at ~ the same time
        let overlap = last_end.saturating_sub(s.timestamp);
        if s.duration >= overlap * 2 {
            html_span_and_children_summary.push(create_span_summary_html(
                root_start_time_unix_micros,
                root_duration_micros,
                s.timestamp,
                s.duration,
                curr_depth,
                &s.name,
            ));
            last_end = s.timestamp + s.duration;
        }
    }
    if !next_layer_spans.is_empty() {
        create_summary_html_span_and_children_single_layer(
            root_start_time_unix_micros,
            root_duration_micros,
            &next_layer_spans,
            Rc::clone(&spans_by_parent_id),
            curr_depth + 1,
            &mut *html_span_and_children_summary,
            max_depth,
        );
    }
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
    root_timestamp: u64,
    root_duration: u64,
    span: &Span,
    depth: i32,
) -> Option<Fragment> {
    let mut span_start = span.timestamp;
    let mut span_duration = span.duration.max(1);
    let root_end = root_timestamp + root_duration;
    if span_start > root_end {
        return None;
    }
    // started before
    if span_start < root_timestamp {
        if root_timestamp < (span_start + span_duration) {
            // needs to exists inside the root range
            let time_started_before_root = root_timestamp - span_start;
            span_start = root_timestamp;
            span_duration -= time_started_before_root;
        } else {
            return None;
        }
    }
    if (span_start + span_duration) > (root_timestamp + root_duration) {
        span_duration = root_end - span_start;
    }
    let start_offset_micros = span_start - root_timestamp;
    let start_offset_percentage: f64 = (100 * start_offset_micros) as f64 / root_duration as f64;
    let duration_percentage: f64 = ((100 * span_duration) as f64 / root_duration as f64).max(0.2);
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
        .filter(|e | e.timestamp <=root_end && e.timestamp >= root_timestamp)
        .map(|e| {
            let k_v: Vec<String> = e
                .key_values
                .iter()
                .filter_map(|kv|  {
                    let k = &kv.key;
                    let v = &kv.value;
                    if kv.user_generated{
                    Some(format!("{k}={v}"))
                    }else{
                        None
                    }
                })
                .collect();
            let k_v = k_v.join(", ");
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
            let event_msg = if !k_v.is_empty() {
                format!("{} {}", e.name, k_v)
            } else {
                format!("{}", e.name)
            };
            // event offset % calculation
            let event_micros_after_trace_start = e.timestamp
                .checked_sub(root_timestamp).unwrap();
            let event_percentage_into_trace_duration =
                100.*event_micros_after_trace_start as f64/ root_duration as f64;
            // don't got over 99.6 because we need to display the character itself too
            let event_percentage_into_trace_duration = event_percentage_into_trace_duration.min(99.6);
            view! {
                <div style="width: 100%; background-color: rgba(255,255,255,0.05)">
                    <p style={format!("margin-left: {event_percentage_into_trace_duration}%")} class="trace-details__event-timestamp">{"|"}</p>
                    <p class="trace-details__event" style={format!("color: {color}")}>{event_msg.to_string()}</p>
                </div>
            }
        })
        .collect();
    let span_k_v: Vec<String> = span
        .key_values
        .iter()
        .filter_map(|kv| {
            let k = &kv.key;
            let v = &kv.value;
            if kv.user_generated {
                Some(format!("{k}={v}"))
            } else {
                None
            }
        })
        .collect();
    let span_k_v = if !span_k_v.is_empty() {
        format!(" - {}", span_k_v.join(", "))
    } else {
        "".to_string()
    };
    let span_with_code_namespace = if let Some(code_namespace) =
        span.key_values.iter().find(|kv| kv.key == "code.namespace")
    {
        format!("{}::{}", code_namespace.value, span.name)
    } else {
        span.name.to_string()
    };
    let span_html = view! {
        <>
        <p class="trace-details__span-name">{format!("{} - {}ms{span_k_v}", span_with_code_namespace, span.duration/1000_000)}</p>
        <div style={format!("margin-left: {start_offset_percentage}%; width: {duration_percentage}%; {}", span_style)}></div>
            {events}
        </>
    };
    Some(span_html)
}
