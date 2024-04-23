use crate::datetime::{printable_local_date, printable_local_date_ms};
use crate::trace::summary::create_summary_html_span_and_children_single_layer;
use crate::TRACE_CHUNK_PATH;
use crate::{API_SERVER_URL_NO_TRAILING_SLASH, PAGE_ROOT_URL};
use api_structs::instance::update::Location;
use api_structs::time_conversion::nanos_to_millis;
use api_structs::ui::trace::search::event::Event;
use api_structs::ui::trace::search::event::TraceEventSearch;
use api_structs::ui::trace::spans::{SingleChunkTraceQuery, Span, TraceChunkId, TraceId};
use api_structs::ui::trace::TraceHeaderAndSpans;
use api_structs::{Env, InstanceId, ServiceId, Severity};
use chrono::Duration;
use js_sys::encode_uri_component;
use js_sys::JSON::stringify;
use leptos::ev::{DOMEventResponder, EventDescriptor};
use leptos::html::Div;
use leptos::leptos_dom::helpers::debounce;
use leptos::{
    component, create_local_resource, event_target_value, html, view, CollectView, Fragment,
    HtmlElement, IntoView, Oco, ReadSignal, Signal, SignalGet, SignalSet, SignalUpdate, SignalWith,
    WriteSignal,
};
use std::collections::HashMap;
use std::ops::Deref;
use std::rc::Rc;
use std::str::FromStr;
use tracing::info;
use wasm_bindgen::JsCast;
use web_sys::{HtmlOptionElement, HtmlSelectElement, MouseEvent, WheelEvent};

mod summary;

fn span_detail(
    trace_header_and_spans: Signal<Option<TraceHeaderAndSpans>>,
    trace_events: Signal<Option<Vec<Event>>>,
    viewing_window_r: ReadSignal<Option<TraceChunkId>>,
    viewing_window_w: WriteSignal<Option<TraceChunkId>>,
) -> Fragment {
    let trace_header_and_spans = trace_header_and_spans.get();
    let trace_header_and_spans = match trace_header_and_spans {
        None => {
            return view! { <><p style="color: white">{format!("Empty, crashed or still loading trace 😅. Check the network tab.")}</p></>};
        }
        Some(spans) if spans.spans.is_empty() => {
            return view! { <><p style="color: white">{format!("Empty trace 😅.")}</p></>};
        }
        Some(spans) => spans,
    };
    let spans = trace_header_and_spans.spans;
    let root = spans
        .iter()
        .find(|d| d.parent_id.is_none())
        .cloned()
        .expect("No parent_span_id is null");
    let trace_start = trace_header_and_spans.start;
    let trace_end = trace_header_and_spans.start + trace_header_and_spans.duration;
    let viewing_window = viewing_window_r.get().unwrap_or_else(|| TraceChunkId {
        start_timestamp: trace_start,
        end_timestamp: trace_end,
    });

    let spans_by_parent_id: HashMap<i64, Vec<Span>> =
        spans
            .clone()
            .into_iter()
            .fold(HashMap::new(), |mut acc, curr| {
                if let Some(parent_id) = curr.parent_id {
                    acc.entry(parent_id).or_default().push(curr);
                }
                acc
            });
    let spans_by_parent_id = Rc::new(spans_by_parent_id);
    let html_span_and_children = move || {
        let viewing_window_w = viewing_window_w;
        let events_html = events_to_html(
            &trace_events.get().unwrap_or_else(|| vec![]),
            trace_start,
            trace_end,
        );
        let mut html_span_and_children_summary = Vec::new();
        let mut max_depth = 0;
        create_summary_html_span_and_children_single_layer(
            viewing_window,
            trace_start,
            trace_end,
            &[root.clone()],
            &spans_by_parent_id,
            0,
            &mut html_span_and_children_summary,
            &mut max_depth,
        );
        let height = max_depth * 20 + 15 + 16; // 16 is my "padding", 8 top, 8 bottom

        let container_ref = leptos::NodeRef::new();
        container_ref.on_load(move |loaded: HtmlElement<leptos::html::Div>| {
            let mut non_passive_wheel = leptos::ev::Custom::<leptos::ev::WheelEvent>::new("wheel");
            let options = non_passive_wheel.options_mut();
            options.passive(false);

            let _div = loaded.on(non_passive_wheel, move |ev| {
                ev.prevent_default();
                let click_x = ev.client_x();
                let container: HtmlElement<Div> =
                    container_ref.get().expect("container to already exist");
                let dom_rect =
                    web_sys::Element::from(container.deref().clone()).get_bounding_client_rect();
                let container_start_x = dom_rect.x();
                let container_width = dom_rect.width();
                let click_x_offset_to_container = click_x as f64 - container_start_x;
                let click_x_offset_percentage =
                    100. * (click_x_offset_to_container / container_width);
                let start_delta = click_x_offset_percentage / 100. * -ev.delta_y() / 3000.;
                let end_delta = (100. - click_x_offset_percentage) / 100. * ev.delta_y() / 3000.;
                let window_start_f64v = viewing_window.start_timestamp as f64;
                let window_end_f64v = viewing_window.end_timestamp as f64;
                let duration =
                    (viewing_window.end_timestamp - viewing_window.start_timestamp) as f64;
                let new_start =
                    (window_start_f64v + start_delta * duration).max(trace_start as f64) as u64;
                let new_end = (window_end_f64v + end_delta * duration).min(trace_end as f64) as u64;
                viewing_window_w.set(Some(TraceChunkId {
                    start_timestamp: new_start,
                    end_timestamp: new_end,
                }));
            });
        });
        let click_handler = move |ev: WheelEvent| {
            ev.prevent_default();
            info!(delta_mode = ev.delta_mode(), "wheel event");
            // let click_x = ev.client_x();
            // let container: HtmlElement<Div> =
            //     container_ref.get().expect("container to already exist");
            // let dom_rect =
            //     web_sys::Element::from(container.deref().clone()).get_bounding_client_rect();
            // let container_start_x = dom_rect.x();
            // let container_width = dom_rect.width();
            // let click_x_offset_to_container = click_x as f64 - container_start_x;
            // let click_x_offset_percentage =
            //     100. * (click_x_offset_to_container as f64 / container_width as f64);
            // let trace_duration = (trace_end - trace_start);
            // let new_start =
            //     trace_start + (trace_duration as f64 * click_x_offset_percentage / 100.) as u64;
            // let current_start = viewing_window_start;
            // let start_change_percentage = 100.
            //     * ((current_start as i64 - new_start as i64).abs() as f64)
            //     / trace_duration as f64;
            //
            // info!("trace_duration={trace_duration}");
            // info!("start_change_percentage={start_change_percentage}");
            // info!("click_x_offset_percentage={click_x_offset_percentage}");
            // info!("current_start={current_start}");
            // info!("new_start={new_start}");
            //
            // let window_duration = viewing_window_end - viewing_window_start;
            // if start_change_percentage <= 3. {
            //     chunk_id_w.set(Some(TraceChunkId {
            //         start_timestamp: current_start,
            //         end_timestamp: current_start + (window_duration as f64 * 2. / 3.) as u64,
            //     }));
            // } else {
            //     chunk_id_w.set(Some(TraceChunkId {
            //         start_timestamp: new_start,
            //         end_timestamp: new_start + (trace_duration - (new_start - trace_start)) as u64, // + (window_duration as f64 * 2. / 3.) as u64,
            //     }));
            // }
        };

        view! {
            <>
                <div  _ref=container_ref style=format!("background-color: rgba(255,255,255,0.05); margin: 15px 0 15px 0; height: {height}px; position: relative")>
                    {html_span_and_children_summary}
                </div>
                {events_html}
            </>
        }
    };

    view! {
        <>
            <div>{html_span_and_children}</div>
        </>
    }
}

#[derive(Clone)]
struct ApiTraceData {
    #[allow(unused)]
    trace_id: TraceId,
    chunk_id: TraceChunkId,
    spans: Vec<Span>,
}

pub struct SelectedSpanOrEvent {
    pub name: String,
    pub key_values: HashMap<String, String>,
    pub location: Location,
}

#[component]
pub fn TraceChunk() -> impl IntoView {
    let query_parameters = leptos_router::use_query_map().get();
    let env = query_parameters.get("env").unwrap().to_string();
    let service_name = query_parameters.get("service_name").unwrap().to_string();
    let instance_id = query_parameters
        .get("instance_id")
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
        instance_id: InstanceId {
            service_id: ServiceId {
                env: Env::from(env.to_string()),
                name: service_name.to_string(),
            },
            instance_id,
        },
        trace_id,
    };
    let (trace_header_and_spans_r, trace_header_and_spans_w) =
        leptos::create_signal(Option::<TraceHeaderAndSpans>::None);
    let (trace_events_r, trace_events_w) = leptos::create_signal(Option::<Vec<Event>>::None);
    let (trace_keys_r, trace_keys_w) = leptos::create_signal(Vec::<String>::new());
    let (trace_events_search_for_r, trace_events_search_for_w) =
        leptos::create_signal(Option::<TraceEventSearch>::None);
    let (viewing_window_r, viewing_window_w) = leptos::create_signal(Option::<TraceChunkId>::None);

    {
        let trace_id = trace_id.clone();
        let _resource = crate::grid::debounced_api(
            Duration::try_milliseconds(2000).unwrap(),
            move || viewing_window_r.get(),
            move |chunk_id: Option<TraceChunkId>| {
                let trace_id = trace_id.clone();
                async move {
                    if let Some(chunk_id) = chunk_id {
                        let query = SingleChunkTraceQuery {
                            trace_id: trace_id.clone(),
                            chunk_id,
                        };
                        let mut trace_events_search_for = trace_events_search_for_r
                            .get()
                            .unwrap_or_else(|| TraceEventSearch {
                                chunk: query.clone(),
                                severity: vec![],
                                substring: None,
                                key_0: None,
                                value_0: None,
                                key_1: None,
                                value_1: None,
                            });
                        trace_events_search_for.chunk = query.clone();
                        trace_events_search_for_w.set(Some(trace_events_search_for));
                    }
                }
            },
        );
    }
    {
        let trace_id = trace_id.clone();
        let _resource = create_local_resource(
            move || trace_events_search_for_r.get(),
            move |search_for: Option<TraceEventSearch>| async move {
                if let Some(search_for) = search_for {
                    get_searched_events(search_for, trace_events_w.clone()).await;
                }
            },
        );
    }

    {
        let trace_id = trace_id.clone();
        let _resource = create_local_resource(
            move || (),
            move |_| {
                let trace_id = trace_id.clone();
                async move {
                    get_trace_keys(trace_id.clone(), trace_keys_w.clone()).await;
                }
            },
        );
    }

    if let (Some(start_timestamp), Some(end_timestamp)) = (start_timestamp, end_timestamp) {
        let chunk = TraceChunkId {
            start_timestamp,
            end_timestamp,
        };
        viewing_window_w.set(Some(chunk.clone()));
    }
    let _api_chunk_list_request_sender = {
        let trace_id = trace_id.clone();
        create_local_resource(move || trace_id.clone(), {
            move |trace_id| {
                get_trace_header_and_spans(trace_id, trace_header_and_spans_w, viewing_window_w)
            }
        })
    };

    let html_spans = move || {
        span_detail(
            Signal::from(trace_header_and_spans_r),
            Signal::from(trace_events_r),
            viewing_window_r,
            WriteSignal::from(viewing_window_w),
        )
    };

    let substring_changed = move |ev: web_sys::Event| {
        let val = event_target_value(&ev);
        info!("Substring changed to: {}", val);
        if val.is_empty() {
            trace_events_search_for_w.update(|v| v.as_mut().unwrap().substring = None);
        } else {
            trace_events_search_for_w.update(|v| v.as_mut().unwrap().substring = Some(val));
        }
    };

    let key_0_changed = move |ev: web_sys::Event| {
        let val = event_target_value(&ev);
        info!("key_0 changed to: {}", val);
        if val.is_empty() {
            trace_events_search_for_w.update(|v| v.as_mut().unwrap().key_0 = None);
        } else {
            trace_events_search_for_w.update(|v| v.as_mut().unwrap().key_0 = Some(val));
        }
    };

    let val_0_changed = move |ev: web_sys::Event| {
        let val = event_target_value(&ev);
        info!("val_0 changed to: {}", val);
        if val.is_empty() {
            trace_events_search_for_w.update(|v| v.as_mut().unwrap().value_0 = None);
        } else {
            trace_events_search_for_w.update(|v| v.as_mut().unwrap().value_0 = Some(val));
        }
    };

    let key_1_changed = move |ev: web_sys::Event| {
        let val = event_target_value(&ev);
        info!("key_1 changed to: {}", val);
        if val.is_empty() {
            trace_events_search_for_w.update(|v| v.as_mut().unwrap().key_1 = None);
        } else {
            trace_events_search_for_w.update(|v| v.as_mut().unwrap().key_1 = Some(val));
        }
    };

    let val_1_changed = move |ev: web_sys::Event| {
        let val = event_target_value(&ev);
        info!("val_1 changed to: {}", val);
        if val.is_empty() {
            trace_events_search_for_w.update(|v| v.as_mut().unwrap().value_1 = None);
        } else {
            trace_events_search_for_w.update(|v| v.as_mut().unwrap().value_1 = Some(val));
        }
    };

    let severity_changed = move |ev: web_sys::Event| {
        let w: HtmlSelectElement = leptos::event_target(&ev);
        let selected_options = w.selected_options();
        let len = selected_options.length();
        let mut severities = vec![];
        for i in 0..len {
            let element: HtmlOptionElement = selected_options
                .get_with_index(i)
                .unwrap()
                .dyn_into()
                .unwrap();
            let value = element.value();
            info!("Severity Value={}", value);
            if let Ok(new) = Severity::from_str(&value) {
                severities.push(new);
            }
        }
        if severities.is_empty() {
            trace_events_search_for_w.update(|v| v.as_mut().unwrap().severity = vec![]);
        } else {
            trace_events_search_for_w.update(|v| v.as_mut().unwrap().severity = severities);
        }
    };
    let event_count_message = move || {
        let event_count = trace_events_r.get().map(|e| e.len()).unwrap_or(0);
        format!("{event_count} Events")
    };
    let a = move || {
        let header_and_spans = match trace_header_and_spans_r.get() {
            None => {
                return view! {
                    <p style="margin: 5px; color: white">{"Loading"}</p>
                };
            }
            Some(header_and_spans) => header_and_spans,
        };

        let name = header_and_spans.top_level_span_name;
        let duration_ms = nanos_to_millis(header_and_spans.duration);

        view! {
            <p style="margin: 5px; color: white">{format!("{} at {} - {name} ({duration_ms}ms)", service_name, env)}</p>
        }
    };
    view! {
        <div class="main-grid">
            <div class="main">
                {a}
                <div class="trace-chunk-list">
                    // {html_chunk_list}
                </div>
                <div class="trace-details">

                    {html_spans}
                </div>
            </div>
            <div>
                <div class="search-panel">
                    <label class="search-panel__label">
                        {event_count_message}
                    </label>
                    <label class="search-panel__label">
                        "Event Message:"
                        <input
                            on:input=substring_changed
                            prop:value={move || trace_events_search_for_r.with(|e| e.as_ref().map(|e| e.substring.clone())).flatten().unwrap_or_default()}
                            class="search-panel__input" type="text" required=true minlength="3" maxlength="20" size="20"
                        />
                    </label>
                    <label class="search-panel__label">
                        "Severity:"
                       <select on:change=severity_changed class="search-panel__input" size=5 multiple>
                             <option value="trace">trace</option>
                             <option value="debug">debug</option>
                             <option value="info">info</option>
                             <option value="warn">warn</option>
                             <option value="error">error</option>
                        </select>
                    </label>
                    <label class="search-panel__label">
                        "Key:"
                        <input on:input=key_0_changed
                            prop:value={move || trace_events_search_for_r.with(|e| e.as_ref().map(|e| e.key_0.clone())).flatten().unwrap_or_default()}
                            class="search-panel__input" type="text"  minlength="3" maxlength="50" size="20"
                            list="trace-key-list"
                        />
                    </label>
                    <label class="search-panel__label">
                        "Val:"
                        <input on:input=val_0_changed
                            prop:value={move || trace_events_search_for_r.with(|e| e.as_ref().map(|e| e.value_0.clone())).flatten().unwrap_or_default()}
                            class="search-panel__input" type="text"  minlength="3" maxlength="50" size="20"
                        />
                    </label>
                    <label class="search-panel__label">
                        "Key:"
                        <input on:input=key_1_changed
                            prop:value={move || trace_events_search_for_r.with(|e| e.as_ref().map(|e| e.key_1.clone())).flatten().unwrap_or_default()}
                            class="search-panel__input" type="text"  minlength="3" maxlength="50" size="20"
                            list="trace-key-list"
                        />
                    </label>
                    <label class="search-panel__label">
                        "Val:"
                        <input on:input=val_1_changed
                            prop:value={move || trace_events_search_for_r.with(|e| e.as_ref().map(|e| e.value_1.clone())).flatten().unwrap_or_default()}
                            class="search-panel__input" type="text"  minlength="3" maxlength="50" size="20"
                        />
                    </label>
                   {move || {
                            let keys = trace_keys_r.get();
                            let spans: Vec<_> = keys.iter().map(|s|{
                                view!{
                                    <option value={s}></option>
                                }
                            }).collect();
                            view!{
                                <datalist id="trace-key-list">
                                  {spans}
                                </datalist>
                            }
                        }
                       }
                </div>
                <div style="margin-top: 20px">
                    <div class="search-panel" style="font-size: small">
                        <p>{"Event or Span Name"}</p>
                        <div style="display: flex">
                            <div style="border: 1px solid #CCCCCC; width: 100px; max-height: 2em; overflow-x: scroll;">
                                <p style="margin: 0">{"Some Big "}</p>
                            </div>
                            <div style="display: grid; align-items: center">
                                "=>"
                            </div>
                            <div style="border: 1px solid #CCCCCC; width: 100px; max-height: 2em; overflow-x: scroll;">
                                <p style="margin: 0">{"VAl! "}</p>
                            </div>
                        </div>
                    </div>
                </div>
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

async fn get_trace_header_and_spans(
    TraceId {
        instance_id,
        trace_id,
    }: TraceId,
    w: WriteSignal<Option<TraceHeaderAndSpans>>,
    window: WriteSignal<Option<TraceChunkId>>,
) {
    info!("Sending req");
    let header_and_spans: TraceHeaderAndSpans = gloo_net::http::Request::get(&format!(
        "{API_SERVER_URL_NO_TRAILING_SLASH}/api/ui/trace/header_and_spans?env={}&name={}&instance_id={}&trace_id={}",
        instance_id.service_id.env,
        instance_id.service_id.name,
        instance_id.instance_id,
        trace_id,
    ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    info!("Got back");
    w.set(Some(header_and_spans.clone()));
    window.set(Some(TraceChunkId {
        start_timestamp: header_and_spans.start,
        end_timestamp: header_and_spans.start + header_and_spans.duration,
    }))
}

async fn get_single_trace(
    SingleChunkTraceQuery {
        trace_id: TraceId {
            instance_id,
            trace_id,
        },
        chunk_id: TraceChunkId {
            start_timestamp,
            end_timestamp,
        },
    }: SingleChunkTraceQuery,
    w: WriteSignal<Option<ApiTraceData>>,
) {
    info!("Sending req");
    let spans: Vec<Span> = gloo_net::http::Request::get(&format!(
        "{API_SERVER_URL_NO_TRAILING_SLASH}/api/ui/trace/chunk?env={}&name={}&instance_id={}&trace_id={}&start_timestamp={start_timestamp}&end_timestamp={end_timestamp}",
        instance_id.service_id.env,
        instance_id.service_id.name,
        instance_id.instance_id,
        trace_id,
    )).send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    info!("Got back");
    let api_trace_data = ApiTraceData {
        trace_id: TraceId {
            instance_id,
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

async fn get_searched_events(search_for: TraceEventSearch, w: WriteSignal<Option<Vec<Event>>>) {
    info!("Sending get_searched_events req");
    info!("search_for={search_for:?}");

    let as_json = serde_json::to_string(&search_for).unwrap();

    let events: Vec<Event> = gloo_net::http::Request::get(&format!(
        "{API_SERVER_URL_NO_TRAILING_SLASH}/api/ui/trace/search?trace_event_search={}",
        as_json
    ))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    info!("Got events back");
    w.set(Some(events));
}

async fn get_trace_keys(trace_id: TraceId, w: WriteSignal<Vec<String>>) {
    info!("Sending get_trace_keys req");
    let events: Vec<String> = gloo_net::http::Request::post(&format!(
        "{API_SERVER_URL_NO_TRAILING_SLASH}/api/ui/trace/keys",
    ))
    .json(&trace_id)
    .unwrap()
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    info!("Got keys back");
    w.set(events);
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

fn events_to_html(events: &[Event], trace_start: u64, trace_end: u64) -> Vec<HtmlElement<Div>> {
    let mut ordered_events = events.to_vec();
    ordered_events.sort_by_key(|e| e.timestamp);
    let events: Vec<_> = ordered_events
        .iter()
        .map(|e| {
            let (event_severity_str, event_color) = match e.severity {
                Severity::Warn => {
                    ("WARN: ", "color: rgb(229, 234, 157)")
                }
                Severity::Error => {
                    ("ERROR: ", "color: rgb(236,103,93)")
                }
                Severity::Trace => {
                    ("TRACE: ", "color: white")
                }
                Severity::Debug => {
                    ("DEBUG: ", "color: white")
                }
                Severity::Info => {
                    ("INFO: ", "color: rgb(137,244,151)")
                }
            };
            let key_values = format_kv(&e.key_values);
            let event_date = printable_local_date_ms(e.timestamp);
            let event_msg = format!(" {}  {}", e.message.as_ref().unwrap_or(&"null".to_string()), key_values);
            // event offset % calculation
            let trace_duration = trace_end - trace_start;
            let event_nanos_after_trace_start = e.timestamp
                .checked_sub(trace_start).unwrap();
            let event_percentage_into_trace_duration =
                100. * event_nanos_after_trace_start as f64 / trace_duration as f64;
            // don't got over 99.6 because we need to display the character itself too
            let event_percentage_into_trace_duration = event_percentage_into_trace_duration.min(99.6);
            view! {
                <div style="width: 100%; background-color: rgba(255,255,255,0.05)">
                    <p style={format!("margin-left: {event_percentage_into_trace_duration}%")} class="trace-details__event-timestamp">{"|"}</p>
                    <p class="trace-details__event" style={"white-space: pre-wrap; color: white"}><span>{event_date}</span><span>"  "</span><span style={event_color}>{event_severity_str}</span>{event_msg}</p>
                </div>
            }
        })
        .collect();
    events
}
fn create_html_span(
    start_timestamp_nanos: u64,
    max_duration: u64,
    span: &Span,
    depth: i32,
) -> Option<Fragment> {
    let span_start = span.timestamp;
    // make it not 0
    let span_duration = span.duration.map(|d| d.max(1));
    // span may start before the start_timestamp_nanos
    let start_offset_nanos = span_start.saturating_sub(start_timestamp_nanos);
    info!("span_start={span_start}");
    info!("start_timestamp_nanos={start_timestamp_nanos}");
    info!("start_offset_nanos={start_offset_nanos}");
    info!("max_duration={max_duration}");
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

    let span_key_vals = format_kv(&span.key_values);
    let span_with_code_namespace = format!(
        "{}::{}",
        span.location.module.as_ref().unwrap_or(&"".to_string()),
        span.name.to_string()
    );
    let span_duration_ms_string = match span.duration {
        None => "still running".to_string(),
        Some(duration) => {
            format!("{}ms", duration / 1000_000)
        }
    };

    let span_html = view! {
        <>
            <p class="trace-details__span-name" style="white-space: pre-wrap">{format!("{} - {span_duration_ms_string} {span_key_vals}", span_with_code_namespace)}</p>
            <div style={format!("margin-left: {start_offset_percentage}%; width: {duration_percentage}%; {}", span_style)}></div>
        </>
    };
    Some(span_html)
}

pub fn format_kv(kv: &HashMap<String, String>) -> String {
    let span_key_vals = kv
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<String>>()
        .join(" ");
    span_key_vals
}
