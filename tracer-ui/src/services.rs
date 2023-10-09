use crate::API_SERVER_URL_NO_TRAILING_SLASH;
use api_structs::exporter::{LiveInstances, NewFiltersRequest};
use api_structs::{Summary, SummaryRequest};
use leptos::html::Input;
use leptos::logging::log;
use leptos::{
    component, create_action, create_node_ref, view, HtmlElement, IntoView, NodeRef, SignalGet,
    SignalSet, WriteSignal,
};

#[component]
pub fn Services(root_path: String) -> impl IntoView {
    let (trace_spans_r, trace_spans_w) = leptos::create_signal(Option::<LiveInstances>::None);
    let _api_request_sender =
        leptos::create_local_resource(move || (), move |_| get_summary(trace_spans_w));
    let change_filters_action = create_action(move |new_filters: &NewFiltersRequest| {
        // `task` is given as `&String` because its value is available in `input`
        update_filter(new_filters.clone())
    });

    let view = move || match trace_spans_r.get() {
        None => {
            view! {
                <div style="padding: 20px; color: white">
                   <p>"Loading, maybe failed, check logs"</p>
                </div>
            }
        }
        Some(instance) if instance.instances.is_empty() => {
            view! {
                <div style="padding: 20px; color: white">
                   <p>"Loaded, but no instances running"</p>
                </div>
            }
        }
        Some(instances) => {
            let instances = instances.instances;
            let mut els = vec![];
            for (service, service_instances) in instances {
                let mut instances = vec![];
                for instance in service_instances {
                    let secs_since_seen = crate::secs_since(instance.last_seen_timestamp);
                    let stats = instance.tracer_stats;
                    let logs_per_minute_limit = stats.sampler_limits.logs_per_minute_limit;
                    let spe_per_minute_limit = stats
                        .sampler_limits
                        .span_plus_event_per_minute_per_trace_limit;
                    let mut html_trace_stats = vec![];
                    for (trace_name, trace_stats) in stats.per_minute_trace_stats {
                        let dropped_traces_per_minute = trace_stats.dropped_traces_per_minute;
                        let spe_usage_per_minute = trace_stats.spe_usage_per_minute;
                        html_trace_stats.push(view!{
                            <p>{format!("{trace_name:-<150} - Usage (SpE/min): {spe_usage_per_minute}/{spe_per_minute_limit} Dropped (Traces/min): {dropped_traces_per_minute}")}</p>
                        });
                    }
                    let input_element: NodeRef<Input> = create_node_ref();

                    let increment = move |_| {
                        change_filters_action.dispatch(NewFiltersRequest {
                            instance_id: instance.service_id,
                            filters: input_element.get().unwrap().value(),
                        });
                    };
                    instances.push(view! {
                    <>
                        <p>{format!("Last seen: {} s ago", secs_since_seen)}</p>
                        <label for="filters">"RUST_LOG Filters: "</label>
                        <input type="text" id="filters" name="filters" node_ref=input_element value={instance.filters} size="100" />
                        <button style="margin-left: 5px" on:click=increment>"Apply"</button>
                        <p>{format!("Logs Usage (events/minute): {}/{} Logs Dropped: {}", stats.orphan_events_per_minute_usage, logs_per_minute_limit, stats.logs_per_minute_dropped)}</p>
                        {html_trace_stats}
                    </>
                });
                }
                els.push(view! {
                    <>
                        <h2>{format!("Service: {service}")}</h2>
                        {instances}
                    </>
                });
            }
            view! {
                <div style="padding: 20px; color: white">
                    {els}
                </div>
            }
        }
    };
    view
}

async fn update_filter(new_filter: NewFiltersRequest) -> Result<(), String> {
    log!("Sending req");
    log!(
        "Updating instance {} to {}",
        new_filter.instance_id,
        new_filter.filters
    );
    let traces = gloo_net::http::Request::post(&format!(
        "{}/api/instances/filter",
        API_SERVER_URL_NO_TRAILING_SLASH
    ))
    .json(&new_filter)
    .unwrap()
    .send()
    .await
    .unwrap()
    .status();
    match traces {
        200 => Ok(()),
        x => Err(format!("Bad status back: {}", x)),
    }
}

async fn get_summary(w: WriteSignal<Option<LiveInstances>>) {
    log!("Sending req");
    let traces: LiveInstances = gloo_net::http::Request::get(&format!(
        "{}/api/instances",
        API_SERVER_URL_NO_TRAILING_SLASH
    ))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    log!("Got summary back");
    w.set(Some(traces));
}
