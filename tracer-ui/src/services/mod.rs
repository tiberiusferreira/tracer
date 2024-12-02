use crate::TrackedGlooError;
use api_structs::Endpoint;
use leptos::prelude::Get;
use leptos::prelude::*;
use tracing::info;

#[component]
pub fn ServiceSummary(mut service: api_structs::ui::service::Service) -> impl IntoView {
    let mut instances = vec![];
    service.instances.sort_by_key(|i| i.last_updated_at);
    service.instances.reverse();
    for i in service.instances {
        let since_registration = chrono::Utc::now().signed_duration_since(i.registered_at);
        let hours_since_registration = since_registration.num_hours();
        let min_since_registration = since_registration.num_minutes() % 60;
        let last_updated_at = match i.last_updated_at {
            None => "Never".to_string(),
            Some(last_updated_at) => {
                let since_update = chrono::Utc::now().signed_duration_since(last_updated_at);
                let hours_since_update = since_update.num_hours();
                let min_since_update = since_update.num_minutes() % 60;
                format!(
                    "Last Seen: {}h {}min ago",
                    hours_since_update, min_since_update
                )
            }
        };
        let log_filter = i.log_filter.unwrap_or_else(|| "<None>".to_string());
        let export_buffer_as_str = i
            .export_buffer_size_bytes
            .map(|e| (e / 1_000).to_string())
            .unwrap_or_else(|| "<None>".to_string());
        instances.push(view! {
            <div style="flex-shrink: 0">
                <p style="margin: 5px 0px;">{format!("Instance: {}", i.id)}</p>
                <p style="margin: 5px 0px;">{format!("Registered: {}h {}min ago", hours_since_registration, min_since_registration)}</p>
                <p style="margin: 5px 0px;">{last_updated_at}</p>
                <p style="margin: 5px 0px;">{format!("Log Filter: {log_filter}")}</p>
                <p style="margin: 5px 0px;">{format!("Export buffer size kb: {export_buffer_as_str}")}</p>
                <p style="margin: 5px 0px;">{format!("Has CPU Profile: {}", i.has_profile_data)}</p>
            </div>
        });
    }
    view! {
        <div>
            <h3>{service.name}</h3>
            <h4>{format!("Log Filter: {}", service.log_filter)}</h4>
            <div id="instance-container" style="display: flex; overflow: auto; gap: 15px">
                {instances}
            </div>
        </div>
    }
}

#[component]
pub fn ServicesContainer(
    service_signal: ReadSignal<
        Option<Result<Vec<api_structs::ui::service::Service>, TrackedGlooError>>,
        LocalStorage,
    >,
) -> impl IntoView {
    move || match service_signal.get() {
        None => (view! {
            <div>
                "Loading"
            </div>
        })
        .into_any(),
        Some(response) => match response {
            Ok(services) => {
                if let Some(a) = services.first() {
                    (view! {
                        <div>
                            <ServiceSummary service=a.clone()/>
                        </div>
                    })
                    .into_any()
                } else {
                    (view! {
                        <div>
                            "Empty"
                        </div>
                    })
                    .into_any()
                }
            }
            Err(e) => {
                let e = tracked_error::error_chain_to_pretty_formatted(e);
                (view! {
                    <div>
                        {format!("Loading error: {e}")}
                    </div>
                })
                .into_any()
            }
        },
    }
}

#[component]
pub fn Services() -> impl IntoView {
    let (service_data_r, service_data_w) = signal_local::<
        Option<Result<Vec<api_structs::ui::service::Service>, TrackedGlooError>>,
    >(None);
    let _api_service_list_request_sender =
        LocalResource::new(move || get_and_write_get_service_data_result(service_data_w));
    view! {
        <div style="padding: 20px; color: white">
            <ServicesContainer service_signal=service_data_r/>
        </div>
    }
}

// fn instance_specific_data_ui(
//     service_id: &ServiceId,
//     instance: &Instance,
//     change_rust_log_action: Action<NewFiltersRequest, Result<(), String>>,
// ) -> leptos::HtmlElement<Div> {
//     let rust_log_ui_input: NodeRef<Input> = create_node_ref();
//     let instance_id = instance.id;
//     let service_id = service_id.clone();
//     let change_rust_log_closure = move |_| {
//         change_rust_log_action.dispatch(NewFiltersRequest {
//             service_id: service_id.clone(),
//             instance_id,
//             log_filter: rust_log_ui_input.get().unwrap().value(),
//         });
//     };
//     let secs_since_seen = instance.last_seen_secs_ago;
//     let instance_rust_log = instance.log_filter.clone();
//     let profile_data = if let Some(profile_data) = &instance.profile_data {
//         let encoded =
//             encode_uri_component(&String::from_utf8(profile_data.profile_data.clone()).unwrap());
//         let profile_download_html_data = format!("data:image/svg+xml,{encoded}");
//         let profile_age = secs_since(profile_data.profile_data_timestamp);
//         view! {
//             <>
//                 <a href=profile_download_html_data download="profile.svg">
//                     {format!("Download Profile - {} s old", profile_age)}
//                 </a>
//             </>
//         }
//     } else {
//         view! { <>"No Profile Data"</> }
//     };
//
//     view! {
//         <div style="display: flex; gap: 20px; justify-content: center; align-items: center">
//             <p style="text-align: center">
//                 {format!("Instance {} Last seen: {} s ago", instance.id, secs_since_seen)}
//             </p>
//             {profile_data}
//             <div style="display: flex; justify-content: center">
//                 <label style="align-self: center" for="filters">
//                     "RUST_LOG Filters: "
//                 </label>
//                 <input
//                     type="text"
//                     id="filters"
//                     name="filters"
//                     node_ref=rust_log_ui_input
//                     value=instance_rust_log
//                     size="70"
//                 />
//                 <button style="margin-left: 5px;" on:click=change_rust_log_closure>
//                     "Apply"
//                 </button>
//             </div>
//         </div>
//     }
// }
//
// fn instance_specific_data_els(
//     service_id: &ServiceId,
//     instances: &[Instance],
// ) -> Vec<HtmlElement<Div>> {
//     let change_rust_log_action = create_action(move |new_filters: &NewFiltersRequest| {
//         send_change_rust_log_http_request(new_filters.clone())
//     });
//     let mut instance_specific_data_els = vec![];
//     for instance in instances {
//         let els = instance_specific_data_ui(service_id, &instance, change_rust_log_action);
//         instance_specific_data_els.push(els);
//     }
//     instance_specific_data_els
// }
//
// fn single_trace_details_els(
//     instances: &[ServiceDataOverTime],
//     alert_config: &AlertConfig,
//     click_timestamp_receiver: WriteSignal<Option<u64>>,
// ) -> HtmlElement<Div> {
//     let (trace_name_show_details_for_r, trace_name_show_details_for_w) =
//         leptos::create_signal(Option::<String>::None);
//     let mut trace_names = HashSet::new();
//     for d in instances {
//         for trace in &d.traces_state {
//             trace_names.insert(trace.trace_name.clone());
//         }
//     }
//     let mut options: Vec<HtmlElement<Option_>> = vec![view! {
//         <option value={""}>{""}</option>
//     }];
//     for name in trace_names {
//         options.push(view! { <option value=name.clone()>{name.clone()}</option> });
//     }
//     let instances = instances.to_vec();
//     let alert_config = alert_config.clone();
//
//     let single_trace_details_graph_els = move || {
//         let Some(trace_name) = trace_name_show_details_for_r.get() else {
//             return view! { <div></div> };
//         };
//         let create_chart_action = create_create_chart_action();
//         let (trace_warning_graph, trace_warning_graph_id): (NodeRef<Div>, String) =
//             graphs::trace_details_graphs::active_finished_warning_error_count::create_graph(
//                 &instances,
//                 trace_name.clone(),
//                 click_timestamp_receiver,
//                 create_chart_action,
//             );
//         let (trace_duration_graph, trace_duration_graph_id): (NodeRef<Div>, String) =
//             graphs::trace_details_graphs::duration::create_graph(
//                 &instances,
//                 trace_name.clone(),
//                 click_timestamp_receiver,
//                 create_chart_action,
//             );
//         // let (trace_warning_percentage_graph, trace_warning_percentage_graph_id): (
//         //     NodeRef<Div>,
//         //     String,
//         // ) = graphs::trace_details_graphs::warning_percentage::create_graph(
//         //     &instances,
//         //     trace_name.clone(),
//         //     alert_config.trace_wide.percentage_check_time_window_secs,
//         //     alert_config.trace_wide.percentage_check_min_number_samples,
//         //     click_timestamp_receiver,
//         //     create_chart_action,
//         // );
//
//         let (budget_usage_graph, budget_usage_graph_id): (NodeRef<Div>, String) =
//             graphs::trace_details_graphs::budget_usage::create_graph(
//                 &instances,
//                 trace_name.clone(),
//                 click_timestamp_receiver,
//                 create_chart_action,
//             );
//
//         view! {
//             <div style="display: flex; flex-wrap: wrap; justify-content: center; margin: 5px 0 5px 0">
//                 <div _ref=trace_warning_graph id=trace_warning_graph_id.clone()></div>
//                 <div _ref=trace_duration_graph id=trace_duration_graph_id.clone()></div>
//                 <div _ref=budget_usage_graph id=budget_usage_graph_id.clone()></div>
//             // <div _ref=spe_usage_per_min_graph id=spe_usage_per_min_graph_id.clone()></div>
//             </div>
//         }
//     };
//     view! {
//         <div style="padding: 20px; color: white">
//             <div style="display: flex; justify-content: center;">
//                 <label for="trace-select">"Select Trace to show details:"</label>
//                 <select
//                     name="trace"
//                     id="trace-select"
//                     on:change=move |e| {
//                         info!("trace-select to {}", event_target_value(&e));
//                         let trace_name: String = event_target_value(&e);
//                         if trace_name.is_empty() {
//                             trace_name_show_details_for_w.set(None);
//                         } else {
//                             trace_name_show_details_for_w.set(Some(trace_name));
//                         }
//                     }
//                 >
//                     {options}
//                 </select>
//             </div>
//             {single_trace_details_graph_els}
//         </div>
//     }
// }
//
// fn single_service_view(service: ServiceOverview) -> leptos::HtmlElement<Div> {
//     let (timestamp_to_show_details_for_r, timestamp_to_show_details_for_w) =
//         leptos::create_signal(Option::<u64>::None);
//     let create_chart_action = create_create_chart_action();
//     let instance_specific_data_els =
//         instance_specific_data_els(&service.service_id, &service.instances);
//
//     let trace_details_els = single_trace_details_els(
//         &service.service_data_over_time,
//         &service.alert_config,
//         timestamp_to_show_details_for_w,
//     );
//
//     let (active_finished_warning_error_count_graph, active_finished_warning_error_count_graph_id): (NodeRef<Div>, String) =
//         graphs::service_graphs::active_finished_warning_error_count::create_graph(
//             &service.service_data_over_time,
//             timestamp_to_show_details_for_w,
//             create_chart_action,
//         );
//     let (max_received_trace_duration_graph, max_received_trace_duration_graph_id): (
//         NodeRef<Div>,
//         String,
//     ) = graphs::service_graphs::max_received_duration::create_graph(
//         &service.service_data_over_time,
//         timestamp_to_show_details_for_w,
//         create_chart_action,
//     );
//
//     let (received_kbytes_graph, received_kbytes_graph_id): (NodeRef<Div>, String) =
//         create_budget_usage_kbytes_graph(
//             &service.service_data_over_time,
//             timestamp_to_show_details_for_w,
//             create_chart_action,
//         );
//
//     let service_name = service.service_id.name.clone();
//     let env = service.service_id.env.clone();
//
//     let alerts_html = alerts::alerts_html(service.alert_config.clone());
//
//     let service_snapshot_html =
//         service_snapshot::get_html(timestamp_to_show_details_for_r, service.clone());
//
//     view! {
//         <div>
//             <h2 style="text-align: center">{format!("Service: {service_name} at {env}")}</h2>
//             {instance_specific_data_els}
//             {service_snapshot_html}
//             {trace_details_els}
//             <p style="text-align: center">{"Service Data:"}</p>
//             <div style="display: flex; flex-wrap: wrap; justify-content: center; margin: 5px 0 5px 0">
//                 <div
//                     _ref=active_finished_warning_error_count_graph
//                     id=active_finished_warning_error_count_graph_id.clone()
//                 ></div>
//                 <div
//                     _ref=max_received_trace_duration_graph
//                     id=max_received_trace_duration_graph_id.clone()
//                 ></div>
//                 <div _ref=received_kbytes_graph id=received_kbytes_graph_id.clone()></div>
//             </div>
//             {alerts_html}
//         </div>
//     }
// }
//
// async fn send_change_rust_log_http_request(new_filter: NewFiltersRequest) -> Result<(), String> {
//     info!(
//         "Sending request to update instance {} to {}",
//         new_filter.instance_id, new_filter.log_filter
//     );
//     let traces = gloo_net::http::Request::post(&format!(
//         "{}/api/ui/service/filter",
//         API_SERVER_URL_NO_TRAILING_SLASH
//     ))
//     .json(&new_filter)
//     .expect("Failed to serialize json")
//     .send()
//     .await
//     .expect("Failed to send request")
//     .status();
//     match traces {
//         200 => {
//             info!("Got 200 response back");
//             Ok(())
//         }
//         x => Err(format!("Bad status back: {}", x)),
//     }
// }
//
// #[instrument(skip_all)]
// pub async fn get_services_list(
//     service_list_w: WriteSignal<Option<Vec<ServiceId>>>,
//     selected_service_w: WriteSignal<Option<ServiceId>>,
// ) {
//     info!("Sending get_services_list req");
//     let list: Vec<ServiceId> = gloo_net::http::Request::get(&format!(
//         "{API_SERVER_URL_NO_TRAILING_SLASH}/api/ui/service/list",
//     ))
//     .send()
//     .await
//     .expect("send to not fail")
//     .json()
//     .await
//     .expect("response to be the expected one");
//     info!(?list, "Got service list back");
//     let first = list.first().cloned();
//     service_list_w.set(Some(list));
//     if let Some(first) = first {
//         selected_service_w.set(Some(first));
//     }
// }
//

async fn get_and_write_get_service_data_result(
    w: WriteSignal<
        Option<Result<Vec<api_structs::ui::service::Service>, TrackedGlooError>>,
        LocalStorage,
    >,
) {
    info!("Sending get_service_data req");
    let res = get_services_impl().await;
    info!("Got get_service_data data back");
    w.set(Some(res));
}

async fn get_services_impl() -> Result<Vec<api_structs::ui::service::Service>, TrackedGlooError> {
    let services = gloo_net::http::Request::get(&format!(
        "{}{}",
        crate::API_SERVER_URL_NO_TRAILING_SLASH,
        api_structs::ui::service::GetService::PATH
    ))
    .send()
    .await?
    .json()
    .await?;
    Ok(services)
}
