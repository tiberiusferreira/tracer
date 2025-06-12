use crate::datetime::{
    printable_local_datetime_hh_mm_chrono, printable_local_datetime_hh_mm_ss_chrono,
};
use crate::error::TrackedGlooError;
use api_structs::execution::Execution;
use api_structs::ui::service::{ExecutionHeader, ExecutionListFilters};
use chrono::{DateTime, Duration, Local, Utc};
use leptos::attr::{height, name, width};
use leptos::html::Div;
use leptos::prelude::*;
use leptos_router::hooks::{use_params, use_params_map};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Display;
use tracing::info;
use uuid::Uuid;
use wasm_bindgen::JsCast;

#[component]
pub fn ExecutionDetailsPage() -> impl IntoView {
    let execution_id = use_params_map()
        .get()
        .get("execution_id")
        .expect("No execution id");
    let execution_id = Uuid::parse_str(&execution_id).expect("invalid execution id");
    let execution_details_data = LocalResource::new(move || get_execution_details(execution_id));

    view! {
        <div>
            <Suspense
                fallback=move || view! { <p>"Loading..."</p> }
            >
                {
                    move || Suspend::new(async move {
                        let execution_details_data = execution_details_data.await;
                        execution_view(execution_details_data)
                    })
                }
            </Suspense>
        </div>
    }
}

#[derive(Debug, Clone)]
struct RenderableIoEvent {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub is_error: bool,
    pub io_provider_name: String,
    pub request: serde_json::Value,
    pub response: Option<serde_json::Value>,
}

fn execution_view(execution: Result<Execution, TrackedGlooError>) -> impl IntoView {
    let execution = match execution {
        Ok(execution) => execution,
        Err(err) => {
            return view! {
                <p style="color: white">{format!("{err:?}")}</p>
            }
            .into_any();
        }
    };
    let duration_ms = (execution.last_seen_at - execution.started_at).num_milliseconds();
    let ended = execution.ended;
    let mut attribute_rows = vec![];
    for attribute in &execution.attributes {
        attribute_rows.push(view! {
            <tr>
                <td class="trace-table__cell">{attribute.name.clone()}</td>
                <td class="trace-table__cell">{attribute.value.clone()}</td>
            </tr>
        });
    }

    let mut renderable_events = vec![];

    for (io_provider_name, events) in &execution.replay_data.io_providers_events {
        for e in events {
            if e.is_response_of.is_some() {
                continue;
            }
            let event_response = events
                .iter()
                .find(|curr| curr.is_response_of == Some(e.id))
                .clone();
            let event_response_is_error = event_response.map(|e| e.is_error).unwrap_or(false);
            let ended_at = event_response
                .map(|curr| curr.created_at)
                .unwrap_or(execution.last_seen_at);
            renderable_events.push(RenderableIoEvent {
                id: e.id,
                created_at: e.created_at,
                ended_at,
                is_error: e.is_error || event_response_is_error,
                io_provider_name: io_provider_name.clone(),
                request: e.value.clone(),
                response: event_response.map(|e| e.value.clone()),
            });
        }
    }
    renderable_events.sort_by_key(|e| e.created_at);
    let selected_event = RwSignal::new(None::<RenderableIoEvent>);
    let execution_start = execution.started_at;
    let execution_end = execution.last_seen_at;
    view! {
        <div style="color: white; margin: 25px">
            <div style="display: grid; grid-template-columns: repeat(2, 1fr); gap: 10px; margin-bottom: 15px">
                <div style="border: 1px solid #444; padding: 10px; border-radius: 4px">
                    <h3 style="margin: 0 0 10px 0">"Execution Info"</h3>
                    <div style="display: grid; grid-template-columns: auto 1fr; gap: 5px">
                        <span>"ID:"</span><span>{execution.id.to_string()}</span>
                        <span>"Environment:"</span><span>{execution.service_env}</span>
                        <span>"Service:"</span><span>{execution.service_name}</span>
                        <span>"Instance:"</span><span>{execution.service_instance_id.to_string()}</span>
                    </div>
                </div>
                <div style="border: 1px solid #444; padding: 10px; border-radius: 4px">
                    <h3 style="margin: 0 0 10px 0">"Timing"</h3>
                    <div>
                        <div>"Started: "{printable_local_datetime_hh_mm_ss_chrono(execution.started_at)}</div>
                        <div>"Duration: "{duration_ms}"ms"</div>
                        <div>"Ended: "{ended}</div>
                        <div>"Size KB: "{execution.size_bytes/1000}</div>
                    </div>
                </div>
            </div>
            <div style="border: 1px solid #444; padding: 10px; border-radius: 4px">
                <h3 style="margin: 0 0 10px 0">"Attributes"</h3>
                <table class="trace-table">
                    {attribute_rows}
                </table>
            </div>
            <details>
                <summary style="font-size: larger; font-weight: bold; cursor: pointer; margin: 5px 0 0 5px">"Replay data"</summary>
                <textarea readonly style="color: white; background-color: black; width: 100%; height: 700px;">
                    {serde_json::to_string_pretty(&execution.replay_data).unwrap()}
                </textarea>
            </details>
            <div style="display: flex; flex-direction: column">
                <TraceView execution_start=execution_start execution_end=execution_end selected_event=selected_event events=renderable_events/>
                <EventDetailsPanel execution_start=execution_start selected_event=selected_event/>
            </div>
        </div>

    }
        .into_any()
}

async fn get_execution_details(id: Uuid) -> Result<Execution, TrackedGlooError> {
    let services = gloo_net::http::Request::get(&format!(
        "{}/api/ui/service/execution?id={id}",
        crate::API_SERVER_URL_NO_TRAILING_SLASH,
    ))
    .send()
    .await?
    .json()
    .await?;
    Ok(services)
}

#[component]
fn TraceView(
    execution_start: DateTime<Utc>,
    execution_end: DateTime<Utc>,
    events: Vec<RenderableIoEvent>,
    selected_event: RwSignal<Option<RenderableIoEvent>>,
) -> impl IntoView {
    let execution_duration_ms =
        u64::try_from((execution_end - execution_start).num_milliseconds()).unwrap();
    let scale_factor = RwSignal::new(0.95);
    let pan_x_offset = RwSignal::new(0.0);
    let pan_y_offset = RwSignal::new(50.0);
    let is_dragging = RwSignal::new(false);
    let last_x = RwSignal::new(0.0);
    let last_y = RwSignal::new(0.0);
    let container_w_h: RwSignal<Option<(i32, i32)>> = RwSignal::new(None);

    let on_wheel = move |e: web_sys::WheelEvent| {
        e.prevent_default();
        // Get mouse position relative to the container
        let container_coordinates_relative_to_viewport = e
            .current_target()
            .expect("event to have a target")
            .dyn_into::<web_sys::Element>()
            .unwrap()
            .get_bounding_client_rect();
        let zoom_delta = -e.delta_y();
        // We want the zoom to be toward the area under the cursor.
        // Normally the zoom will stretch the content as a whole, zooming toward the center of the content, shifting
        // the edges of the content by the same amount on each side.
        //
        // If we shift it right by the amount that it would be normally shifted by the zoom, we can keep any part stable.
        // The amount to shift to keep the left edge stable is half the width of the content change.
        // To keep the center stable, there is no need to shift at all.
        // To keep the middle point between the center and the left stable, we shift it by halfway between the amount
        // to keep the center stable and keep the left stable: 1/4 of the width change.
        //
        // So to keep the area under the cursor stable, we shift the content account to its position off-center.
        // If it's in the center: no shift at all.
        // If it's to the left: shift by half the width change.
        // If it's somewhere in-between, we shift it proportionally.
        //
        //
        // The content initially takes the whole container
        let original_width = container_coordinates_relative_to_viewport.width();
        let current_scale = scale_factor.get_untracked();
        let new_scale = (current_scale * (1.0 + zoom_delta / 1000.0)).clamp(0.1, 10.0);
        let current_width = original_width * current_scale;
        let new_width = original_width * new_scale;
        let width_delta = new_width - current_width;
        // Now we need to find where the cursor is relative to the content center.
        // The center is initially at width/2.
        // [---------C--------]
        // [       width      ]
        let curr_pan_x_offset = pan_x_offset.get_untracked();
        let center = (original_width / 2.) + curr_pan_x_offset;
        let container_left = container_coordinates_relative_to_viewport.left();
        let mouse_left = e.client_x() as f64;
        let mouse_left_relative_container = mouse_left - container_left;
        // [   M     C        ]
        let mouse_to_center_distance = center - mouse_left_relative_container;
        let mouse_to_center_proportional = mouse_to_center_distance / (current_width / 2.);
        let new_offset = mouse_to_center_proportional * (width_delta / 2.);

        pan_x_offset.update(|offset| {
            let new_pan_x_offset = (*offset + new_offset);
            *offset = new_pan_x_offset;
        });
        scale_factor.set(new_scale);
    };

    let on_mouse_down = move |e: web_sys::MouseEvent| {
        is_dragging.set(true);
        last_x.set(e.client_x() as f64);
        last_y.set(e.client_y() as f64);
    };

    let on_mouse_move = move |e: web_sys::MouseEvent| {
        if is_dragging.get() {
            let dx = e.client_x() as f64 - last_x.get();
            pan_x_offset.update(|offset| *offset = *offset + dx);
            last_x.set(e.client_x() as f64);
            let dy = e.client_y() as f64 - last_y.get();
            pan_y_offset.update(|offset| *offset = *offset + dy);
            last_y.set(e.client_y() as f64);
        }
    };

    let on_mouse_up = move |_| {
        is_dragging.set(false);
    };
    // let container_width = 1666i64;
    // let container_height = 620i64;
    let container_ref = NodeRef::<Div>::new();

    Effect::new(move |_| {
        if let Some(el) = container_ref.get() {
            let el = el.clone();
            let closure = wasm_bindgen::closure::Closure::<dyn FnMut()>::new(move || {
                let width = el.client_width();
                let height = el.client_height();
                info!("Measured via RAF: {width} x {height}");
                container_w_h.set(Some((width, height)));
            });
            window()
                .request_animation_frame(closure.as_ref().unchecked_ref())
                .expect("requestAnimationFrame failed");
            closure.forget(); // leak is OK for one-shot
        }
    });

    let style_fun = move || {
        let pan_x = pan_x_offset.get() as i64;
        let pan_y = pan_y_offset.get() as i64;
        let scale = scale_factor.get();
        format!("transform: translate({pan_x}px, {pan_y}px) scale({scale}, {scale})")
    };
    view! {
        <div
            node_ref=container_ref
            style="position: relative; margin: 5px;  border: 1px solid white; height: 300px; overflow: hidden; resize: vertical; cursor: grab"
            on:wheel=on_wheel
            on:mousedown=on_mouse_down
            on:mousemove=on_mouse_move
            on:mouseup=on_mouse_up
            on:mouseleave=on_mouse_up
        >
            <div style=style_fun>
                {
                    let events = events.clone();
                    move || {
                        let Some((container_width, container_height)) = container_w_h.get() else{
                            return vec![];
                        };
                        events.iter().enumerate().map(|(i, event)| {
                                let color = if i%2 ==0{
                                    "hsl(220, 60%, 60%)"
                                }else{
                                    "hsl(190, 50%, 45%)"
                                };
                                single_event_view(execution_start, execution_duration_ms, i, color.to_string(), event.clone(), selected_event.clone(), container_width, container_height)
                        })
                        .collect::<Vec<_>>()
                    }
                }
                {
                    move || {
                        let Some((container_width, container_height)) = container_w_h.get() else{
                            return vec![];
                        };
                        {timeline_markers(execution_duration_ms, container_width)}
                    }
                }
            </div>
        </div>
    }
}

fn timeline_markers(execution_duration_ms: u64, container_width: i32) -> Vec<impl IntoView> {
    let mut curr_ms = 0;
    let mut els = vec![];
    let width = 1.0;
    let top = 20;
    let height = 20.0;
    loop {
        if (curr_ms + 100) > execution_duration_ms {
            break;
        }
        let relative_x = curr_ms as f64 / execution_duration_ms as f64;
        let left = relative_x * container_width as f64;
        let style_f = format!(
            "position: absolute; height: {}px; left: {}px; width: {}px; top: {}px; background-color: {}; transition: none;",
            height, left, width, top, "white"
        );
        els.push(view! {
            <div style={style_f}>
                <span style="margin-left: 5px; font-size: small">
                    {format!(" {curr_ms}ms")}
                </span>
            </div>
        });
        curr_ms += 100;
    }
    let style_f = format!(
        "position: absolute; height: {}px; left: {}px; width: {}px; top: {}px; background-color: {}; transition: none;",
        height, container_width, width, top, "white"
    );
    els.push(view! {
        <div style={style_f}>
            <span style="margin-left: -45px; font-size: small">
                {format!(" {execution_duration_ms}ms")}
            </span>
        </div>
    });
    els
}

fn single_event_view(
    execution_start: DateTime<Utc>,
    execution_duration_ms: u64,
    index: usize,
    color: String,
    event: RenderableIoEvent,
    selected_event: RwSignal<Option<RenderableIoEvent>>,
    // scale_factor: f64,
    // pan_x: f64,
    // pan_y: f64,
    container_width: i32,
    container_height: i32,
) -> impl IntoView {
    let event_start_ms = (event.created_at - execution_start).num_milliseconds() as f64;
    let event_duration_ms = (event.ended_at - event.created_at).num_milliseconds() as f64;
    // if event starts at 1/3 of the execution, we would have 0.3333 here
    let event_start_relative_to_execution_percentage =
        event_start_ms / execution_duration_ms as f64;
    let event_duration_relative_to_execution_percentage =
        event_duration_ms / execution_duration_ms as f64;
    let left = event_start_relative_to_execution_percentage * container_width as f64;
    let width = event_duration_relative_to_execution_percentage * container_width as f64;
    let is_selected =
        Signal::derive(move || selected_event.get().as_ref().map(|e| e.id) == Some(event.id));
    let top = 60;
    let top_tooltip = 15;
    let height = 30.0;
    let tooltip_style = format!("position: absolute; left: {left}px; top: {top_tooltip}px");
    view! {
        <div>
            <div
                style=move || {
                    let bg = if event.is_error {
                        "red"
                    } else if selected_event.get().as_ref().map(|e| e.id) == Some(event.id) {
                        "#007bff"
                    } else {
                        color.as_str()
                    };


                    format!(
                        "position: absolute; height: {}px; border-radius: 2px; cursor: pointer; \
                        left: {}px; width: {}px; top: {}px; background-color: {}; transition: none;",
                        height, left, width, top, bg
                    )

                }
                on:click=move |_| selected_event.update(|prev| {
                    if prev.as_ref().map(|e| e.id) == Some(event.id) {
                        *prev = None
                    } else {
                        *prev = Some(event.clone())
                    }
                })
            >

                <span style="font-size: medium">
                    {
                        if event_duration_ms > 35.{
                            format!("{} ms", event_duration_ms)
                        }else{
                            "".to_string()
                        }
                    }
                </span>
            </div>
            <span class="my_tooltip" style={tooltip_style}>
                {format!("{} ms", event_duration_ms)}
            </span>
        </div>

    }
}

#[component]
fn EventDetailsPanel(
    execution_start: DateTime<Utc>,
    selected_event: RwSignal<Option<RenderableIoEvent>>,
) -> impl IntoView {
    view! {
        <div style="height: 300px; margin:5px; border: 1px solid #ccc; overflow: scroll; resize: vertical;">
            {move || {
                if let Some(event) = selected_event.get() {

                    let offset_in_execution_ms = (event.created_at - execution_start).num_milliseconds();
                    let duration_ms = (event.ended_at - event.created_at).num_milliseconds();
                    view! {
                        <div>
                            <h2>"Event Details"</h2>
                            <p><strong>"Provider: "</strong>{event.io_provider_name.to_string()}</p>
                            <p>{format!("Start: {} - {offset_in_execution_ms}ms in execution - {duration_ms}ms", printable_local_datetime_hh_mm_ss_chrono(event.created_at))}</p>
                            <p><strong>"Error: "</strong>{event.is_error.to_string()}</p>
                            <h3>"Request:"</h3>
                            <pre>{serde_json::to_string_pretty(&event.request).unwrap_or_default().replace("\\n","\n")}</pre>
                            <h3>"Response:"</h3>
                            <pre>{
                                event.response
                                    .as_ref()
                                    .map(|r| serde_json::to_string_pretty(r).unwrap_or_default().replace("\\n", "\n"))
                                    .unwrap_or_else(|| "None".to_string())
                            }</pre>
                        </div>
                    }.into_any()
                } else {
                    view! { <p>"Click an event to see details."</p> }.into_any()
                }
            }}
        </div>
    }
}
