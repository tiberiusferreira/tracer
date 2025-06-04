use crate::error::TrackedGlooError;
use api_structs::execution::Execution;
use api_structs::ui::service::{ExecutionHeader, ExecutionListFilters};
use chrono::{DateTime, Duration, Local, Utc};
use leptos::prelude::*;
use leptos_router::hooks::{use_params, use_params_map};
use std::collections::HashMap;
use std::fmt::Display;
use tracing::info;
use uuid::Uuid;

#[component]
pub fn ExecutionDetails() -> impl IntoView {
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
                    a(execution_details_data)
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

fn a(execution: Result<Execution, TrackedGlooError>) -> impl IntoView {
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
    view! {
        <div style="color: white">
            <p>{format!("Id = {}", execution.id.to_string())}</p>
            <p>{format!("Env = {}", execution.service_env)}</p>
            <p>{format!("Service = {}", execution.service_name)}</p>
            <p>{format!("Instance = {}", execution.service_instance_id)}</p>
            <p>{format!("Started: {} - {duration_ms}ms - ended: {ended}", execution.started_at.with_timezone(&Local).format("%d/%m/%Y %H:%M").to_string())}</p>
            <table class="trace-table">
                <tr>
                    <th colspan="2" class="trace-table__cell">"Attributes"</th>
                </tr>
                {attribute_rows}
            </table>
            <details>
                <summary style="font-size: larger; font-weight: bold; cursor: pointer; margin: 5px 0 0 5px">"Replay data"</summary>
                <textarea readonly style="color: white; background-color: black; width: 100%; height: 700px;">
                    {serde_json::to_string_pretty(&execution.replay_data).unwrap()}
                </textarea>
            </details>
            <TraceView selected_event=selected_event events=renderable_events/>
            <EventDetailsPanel selected_event=selected_event/>
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
    selected_event: RwSignal<Option<RenderableIoEvent>>,
    events: Vec<RenderableIoEvent>,
) -> impl IntoView {
    let min_time = events.iter().map(|e| e.created_at).min().unwrap();

    view! {
        <div class="trace-container">
            {events.into_iter().enumerate().map(|(i, event)| {
                let offset = (event.created_at - min_time).num_milliseconds() as f64;
                let duration = (event.ended_at - event.created_at).num_milliseconds() as f64;

                let left = offset * 2.0; // scale factor for display
                let width = duration * 2.0;

                let top = i as f64 * 30.0;

                view! {
                    <div
                        class="trace-bar"
                        style=format!("left: {}px; width: {}px; top: {}px; background-color: {};",
                            left, width, top,
                            if event.is_error { "red" } else { "green" }
                        )
                        on:click=move |_| selected_event.set(Some(event.clone()))
                    >
                        <span class="tooltip">
                            {format!("{} ({} ms)", event.io_provider_name, duration)}
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
        <div class="event-details">
            {move || {
                if let Some(event) = selected_event.get() {
                    view! {
                        <div>
                            <h2>"Event Details"</h2>
                            <p><strong>"Provider: "</strong>{event.io_provider_name.to_string()}</p>
                            <p><strong>"Start: "</strong>{event.created_at.to_rfc3339()}</p>
                            <p><strong>"End: "</strong>{event.ended_at.to_rfc3339()}</p>
                            <p><strong>"Error: "</strong>{event.is_error.to_string()}</p>
                            <h3>"Request:"</h3>
                            <pre>{serde_json::to_string_pretty(&event.request).unwrap_or_default()}</pre>
                            <h3>"Response:"</h3>
                            <pre>{
                                event.response
                                    .as_ref()
                                    .map(|r| serde_json::to_string_pretty(r).unwrap_or_default())
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
