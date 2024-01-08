use crate::services::graph_creation::create_create_chart_action;
use crate::services::graphs::{
    create_active_traces_graph, create_dropped_traces_by_sampling_per_min_graph,
    create_orphan_events_dropped_by_sampling_per_minute_graph,
    create_orphan_events_per_minute_usage_graph, create_received_orphan_event_bytes_graph,
    create_received_spe_graph, create_received_trace_kbytes_graph,
    create_spe_buffer_usage_traces_graph, create_spe_dropped_due_to_full_export_buffer_graph,
    create_trace_spe_usage_traces_graph,
};
use crate::{secs_since, API_SERVER_URL_NO_TRAILING_SLASH};
use api_structs::ui::service_health::{Instance, ServiceData, ServiceId};
use api_structs::ui::NewFiltersRequest;
use js_sys::encode_uri_component;
use leptos::html::{Div, Input};
use leptos::logging::log;
use leptos::{
    component, create_action, create_node_ref, event_target_value, view, Action, IntoView, NodeRef,
    SignalGet, SignalSet, WriteSignal,
};
mod active_traces_table;
mod alerts;
mod graph_creation;
mod graphs;

#[component]
pub fn ServicesStatistics(page_root_url: String) -> impl IntoView {
    let (service_data_r, service_data_w) = leptos::create_signal(Option::<ServiceData>::None);
    let (selected_service_r, selected_service_w) = leptos::create_signal(Option::<ServiceId>::None);
    let (service_list_r, service_list_w) = leptos::create_signal(Option::<Vec<ServiceId>>::None);
    let _api_service_list_request_sender = leptos::create_local_resource(
        move || (),
        move |_| get_services_list(service_list_w, selected_service_w),
    );
    let _api_service_data_request_sender = leptos::create_local_resource(
        move || selected_service_r.get(),
        move |selected_service| async move {
            if let Some(selected_service) = selected_service {
                get_service_data(selected_service, service_data_w).await;
            }
        },
    );

    let view = move || match service_list_r.get() {
        None => {
            view! {
                <div style="padding: 20px; color: white">
                   <p>"Loading, maybe failed, check console log :D"</p>
                </div>
            }
        }
        Some(services) if services.is_empty() => {
            view! {
                <div style="padding: 20px; color: white">
                   <p>"Loaded, but no services running"</p>
                </div>
            }
        }
        Some(services) => {
            let mut options = vec![];
            for (idx, service) in services.iter().enumerate() {
                let service_name_and_env = format!("{} at {}", service.name, service.env);
                options.push(view! {
                    <option value={idx}>{service_name_and_env}</option>
                });
            }
            let page_root_url = page_root_url.clone();
            let service_data_html = move || match service_data_r.get() {
                None => {
                    view! {
                        <div>
                        </div>
                    }
                }
                Some(service_data) => single_service_view(page_root_url.clone(), service_data),
            };

            view! {
                <div style="padding: 20px; color: white">
                    <label for="service-select">"Select Service:"</label>
                    <select name="service" id="service-select" on:change={move |e| {
                            log!("changed to {}", event_target_value(&e));
                            let id: usize = event_target_value(&e).parse().unwrap();
                            selected_service_w.set(Some(services[id].clone()));
                        }
                    }>
                    {options}
                    </select>
                    {service_data_html}
                </div>
            }
        }
    };
    view
}

fn services_view(page_root_url: String, services: Vec<ServiceData>) -> leptos::HtmlElement<Div> {
    let mut services_els = vec![];
    for service in services {
        services_els.push(single_service_view(page_root_url.clone(), service));
    }
    view! {
        <div style="padding: 20px; color: white">
            {services_els}
        </div>
    }
}

fn instance_specific_data_ui(
    instance: &Instance,
    change_rust_log_action: Action<NewFiltersRequest, Result<(), String>>,
) -> leptos::HtmlElement<Div> {
    let rust_log_ui_input: NodeRef<Input> = create_node_ref();
    let instance_id = instance.id;
    let change_rust_log_closure = move |_| {
        change_rust_log_action.dispatch(NewFiltersRequest {
            instance_id,
            filters: rust_log_ui_input.get().unwrap().value(),
        });
    };
    let secs_since_seen = instance
        .time_data_points
        .last()
        .map(|i| secs_since(i.timestamp))
        .unwrap_or(9999);
    let instance_rust_log = instance.rust_log.clone();
    let profile_data = if let Some(profile_data) = &instance.profile_data {
        let encoded =
            encode_uri_component(&String::from_utf8(profile_data.profile_data.clone()).unwrap());
        let profile_download_html_data = format!("data:image/svg+xml,{encoded}");
        let profile_age = secs_since(profile_data.profile_data_timestamp);
        view! {
            <>
                <a href={profile_download_html_data} download="profile.svg">{format!("Download Profile - {} s old", profile_age)}</a>
            </>
        }
    } else {
        view! {
            <>
                "No Profile Data"
            </>
        }
    };

    view! {
        <div style="display: flex; gap: 20px; justify-content: center; align-items: center">
                <p style="text-align: center">{format!("Instance {} Last seen: {} s ago", instance.id, secs_since_seen)}</p>
                {profile_data}
                <div style="display: flex; justify-content: center">
                    <label style="align-self: center" for="filters">"RUST_LOG Filters: "</label>
                    <input type="text" id="filters" name="filters" node_ref=rust_log_ui_input value={instance_rust_log} size="70" />
                    <button style="margin-left: 5px;" on:click=change_rust_log_closure>"Apply"</button>
                </div>
        </div>
    }
}

fn instance_specific_data_els(instances: &[Instance]) -> Vec<leptos::HtmlElement<Div>> {
    let change_rust_log_action = create_action(move |new_filters: &NewFiltersRequest| {
        send_change_rust_log_http_request(new_filters.clone())
    });
    let mut instance_specific_data_els = vec![];
    for instance in instances {
        let els = instance_specific_data_ui(&instance, change_rust_log_action);
        instance_specific_data_els.push(els);
    }
    instance_specific_data_els
}

fn single_service_view(page_root_url: String, service: ServiceData) -> leptos::HtmlElement<Div> {
    let (timestamp_to_show_details_for_r, timestamp_to_show_details_for_w) =
        leptos::create_signal(Option::<u64>::None);
    let create_chart_action = create_create_chart_action();
    let instance_specific_data_els = instance_specific_data_els(&service.instances);

    let (active_traces_graph, active_traces_graph_id): (NodeRef<Div>, String) =
        create_active_traces_graph(
            &service.instances,
            timestamp_to_show_details_for_w,
            create_chart_action,
        );
    let (spe_buffer_usage, spe_buffer_usage_graph_id): (NodeRef<Div>, String) =
        create_spe_buffer_usage_traces_graph(
            &service.instances,
            timestamp_to_show_details_for_w,
            create_chart_action,
        );

    let (trace_spe_usage, trace_spe_usage_graph_id): (NodeRef<Div>, String) =
        create_trace_spe_usage_traces_graph(
            &service.instances,
            timestamp_to_show_details_for_w,
            create_chart_action,
        );

    let (received_spe_graph, received_spe_graph_id): (NodeRef<Div>, String) =
        create_received_spe_graph(
            &service.instances,
            timestamp_to_show_details_for_w,
            create_chart_action,
        );

    let (received_trace_kbytes_graph, received_trace_kbytes_graph_id): (NodeRef<Div>, String) =
        create_received_trace_kbytes_graph(
            &service.instances,
            timestamp_to_show_details_for_w,
            create_chart_action,
        );

    let (received_orphan_event_bytes_graph, received_orphan_event_bytes_graph_id): (
        NodeRef<Div>,
        String,
    ) = create_received_orphan_event_bytes_graph(
        &service.instances,
        timestamp_to_show_details_for_w,
        create_chart_action,
    );

    let (orphan_events_per_minute_usage_graph, orphan_events_per_minute_usage_graph_id): (
        NodeRef<Div>,
        String,
    ) = create_orphan_events_per_minute_usage_graph(
        &service.instances,
        timestamp_to_show_details_for_w,
        create_chart_action,
    );
    let (dropped_traces_by_sampling_per_min_graph, dropped_traces_by_sampling_per_min_graph_id): (
        NodeRef<Div>,
        String,
    ) = create_dropped_traces_by_sampling_per_min_graph(
        &service.instances,
        timestamp_to_show_details_for_w,
        create_chart_action,
    );

    let (
        spe_dropped_due_to_full_export_buffer_graph,
        spe_dropped_due_to_full_export_buffer_graph_id,
    ): (NodeRef<Div>, String) = create_spe_dropped_due_to_full_export_buffer_graph(
        &service.instances,
        timestamp_to_show_details_for_w,
        create_chart_action,
    );

    let (
        orphan_events_dropped_by_sampling_per_minute_graph,
        orphan_events_dropped_by_sampling_per_minute_graph_id,
    ): (NodeRef<Div>, String) = create_orphan_events_dropped_by_sampling_per_minute_graph(
        &service.instances,
        timestamp_to_show_details_for_w,
        create_chart_action,
    );

    let service_name = service.name.clone();
    let env = service.env;

    let alerts_html = alerts::alerts_html(service.alert_config.clone(), page_root_url.clone());

    let active_services_html = active_traces_table::active_traces_table_html(
        timestamp_to_show_details_for_r,
        service,
        page_root_url.clone(),
    );
    view! {
        <div>
            <h2 style="text-align: center">{format!("Service: {service_name} at {env}")}</h2>
                {instance_specific_data_els}
                {active_services_html}
                <div style="display: flex; flex-wrap: wrap; justify-content: center; margin: 5px 0 5px 0">
                    <div _ref=active_traces_graph id=active_traces_graph_id.clone()></div>
                    <div _ref=trace_spe_usage id=trace_spe_usage_graph_id.clone()></div>
                    <div _ref=orphan_events_per_minute_usage_graph id=orphan_events_per_minute_usage_graph_id.clone()></div>
                    <div _ref=received_trace_kbytes_graph id=received_trace_kbytes_graph_id.clone()></div>
                    <div _ref=received_orphan_event_bytes_graph id=received_orphan_event_bytes_graph_id.clone()></div>
                    <div _ref=received_spe_graph id=received_spe_graph_id.clone()></div>
                    <div _ref=spe_buffer_usage id=spe_buffer_usage_graph_id.clone()></div>
                    <div _ref=dropped_traces_by_sampling_per_min_graph id=dropped_traces_by_sampling_per_min_graph_id.clone()></div>
                    <div _ref=spe_dropped_due_to_full_export_buffer_graph id=spe_dropped_due_to_full_export_buffer_graph_id.clone()></div>
                    <div _ref=orphan_events_dropped_by_sampling_per_minute_graph id=orphan_events_dropped_by_sampling_per_minute_graph_id.clone()></div>
                </div>
                {alerts_html}

        </div>
    }
}

async fn send_change_rust_log_http_request(new_filter: NewFiltersRequest) -> Result<(), String> {
    log!(
        "Sending request to update instance {} to {}",
        new_filter.instance_id,
        new_filter.filters
    );
    let traces = gloo_net::http::Request::post(&format!(
        "{}/api/instances/filter",
        API_SERVER_URL_NO_TRAILING_SLASH
    ))
    .json(&new_filter)
    .expect("Failed to serialize json")
    .send()
    .await
    .expect("Failed to send request")
    .status();
    match traces {
        200 => {
            log!("Got 200 response back");
            Ok(())
        }
        x => Err(format!("Bad status back: {}", x)),
    }
}

async fn get_services_list(
    w: WriteSignal<Option<Vec<ServiceId>>>,
    selected_service_w: WriteSignal<Option<ServiceId>>,
) {
    log!("Sending get_services_list req");
    let list: Vec<ServiceId> = gloo_net::http::Request::get(&format!(
        "{}/api/service/list",
        API_SERVER_URL_NO_TRAILING_SLASH
    ))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    log!("Got service list back");
    let first = list.first().cloned();
    w.set(Some(list));
    if let Some(first) = first {
        selected_service_w.set(Some(first));
    }
}

async fn get_service_data(service_id: ServiceId, w: WriteSignal<Option<ServiceData>>) {
    log!("Sending get_service_data req");
    let traces: ServiceData = gloo_net::http::Request::get(&format!(
        "{}/api/service/data/{}/{}",
        API_SERVER_URL_NO_TRAILING_SLASH, service_id.name, service_id.env
    ))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    log!("Got get_service_data data back");
    w.set(Some(traces));
}
