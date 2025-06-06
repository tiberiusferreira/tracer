use crate::datetime::{
    printable_local_datetime_hh_mm_chrono, printable_local_datetime_hh_mm_ss_chrono,
};
use crate::error::TrackedGlooError;
use api_structs::execution::Execution;
use api_structs::ui::service::{ExecutionHeader, ExecutionListFilters};
use chrono::{DateTime, Duration, Local, Utc};
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
    info!("execution_id = {execution_id}");
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
    let start = execution.started_at;
    let end = execution.last_seen_at;
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
                <TraceView2 start=start end=end selected_event=selected_event events=renderable_events/>
                <EventDetailsPanel selected_event=selected_event/>
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
fn TraceView2(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    events: Vec<RenderableIoEvent>,
    selected_event: RwSignal<Option<RenderableIoEvent>>,
) -> impl IntoView {
    let scale_factor = RwSignal::new(2.0); // Initial scale factor (was hardcoded as 2.0)
    let pan_x_offset = RwSignal::new(0.0); // Tracks horizontal panning
    let pan_y_offset = RwSignal::new(0.0); // Tracks vertical panning
    let is_dragging = RwSignal::new(false);
    let last_x = RwSignal::new(0.0);
    let last_y = RwSignal::new(0.0);

    let on_wheel = move |e: web_sys::WheelEvent| {
        e.prevent_default();

        // Get mouse position relative to the container
        let rect = e
            .target()
            .expect("Event should have a target")
            .dyn_into::<web_sys::Element>()
            .unwrap()
            .get_bounding_client_rect();

        let mouse_x = e.client_x() as f64 - rect.left() - pan_x_offset.get();
        let mouse_y = e.client_y() as f64 - rect.top() - pan_y_offset.get();

        let delta = -e.delta_y();
        let old_scale = scale_factor.get();
        let new_scale = (old_scale * (1.0 + delta / 1000.0)).clamp(0.1, 10.0);

        // Calculate the position change
        let scale_change = new_scale - old_scale;
        let dx = -mouse_x * (scale_change / old_scale);
        let dy = -mouse_y * (scale_change / old_scale);

        scale_factor.set(new_scale);
        pan_x_offset.update(|offset| *offset = *offset + dx);
        pan_y_offset.update(|offset| *offset = *offset + dy);
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

    view! {
        <div
            style="position: relative; margin: 5px; height: 300px; border: 1px solid white; overflow: hidden; resize: vertical; cursor: grab"
            on:wheel=on_wheel
            on:mousedown=on_mouse_down
            on:mousemove=on_mouse_move
            on:mouseup=on_mouse_up
            on:mouseleave=on_mouse_up
        >
            {events.into_iter().enumerate().map(|(i, event)| {
                let x_offset = (event.created_at - start).num_milliseconds() as f64;
                let duration = (event.ended_at - event.created_at).num_milliseconds() as f64;
                let is_selected = Signal::derive(move || selected_event.get().as_ref().map(|e| {
                    e.id
                }) == Some(event.id));

                view! {
                    <div
                        style=move || {
                            let bg = if event.is_error {
                                "red"
                            } else if selected_event.get().as_ref().map(|e| e.id) == Some(event.id) {
                                "#007bff"
                            } else {
                                "green"
                            };

                            let left = x_offset * scale_factor.get() + pan_x_offset.get();
                            let width = duration * scale_factor.get();
                            let spacing = 2.0 * scale_factor.get();
                            let top = i as f64 * spacing + pan_y_offset.get();
                            let height = 20.0 * scale_factor.get();

                            format!(
                                "position: absolute; height: {}px; border-radius: 4px; cursor: pointer; \
                                left: {}px; width: {}px; top: {}px; background-color: {}; \
                                transition: none;",
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
                        // <span class="my_tooltip">
                        //     {format!("{} ms", duration)}
                        // </span>
                        <span>
                            {
                                if duration > 30.{
                                    format!("{} ms", duration)
                                }else{
                                    "".to_string()
                                }
                            }
                        </span>
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}

#[component]
fn EventDetailsPanel(selected_event: RwSignal<Option<RenderableIoEvent>>) -> impl IntoView {
    view! {
        <div style="height: 300px; margin:5px; border: 1px solid #ccc; overflow: scroll; resize: vertical;">
            {move || {
                if let Some(event) = selected_event.get() {
                    let duration_ms = (event.ended_at - event.created_at).num_milliseconds();
                    view! {
                        <div>
                            <h2>"Event Details"</h2>
                            <p><strong>"Provider: "</strong>{event.io_provider_name.to_string()}</p>
                            <p>{format!("Start: {} - {duration_ms}ms", printable_local_datetime_hh_mm_ss_chrono(event.created_at))}</p>
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
