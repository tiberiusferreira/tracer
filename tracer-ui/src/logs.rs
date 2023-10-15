use crate::{printable_local_date, API_SERVER_URL_NO_TRAILING_SLASH};
use api_structs::exporter::{Log, NewFiltersRequest, ServiceLogRequest, ServiceNameList, Severity};
use leptos::logging::log;
use leptos::{component, view, CollectView, IntoView, SignalGet, SignalSet, WriteSignal};

#[component]
pub fn Logs(root_path: String) -> impl IntoView {
    let (service_name_list_r, service_name_list_w) =
        leptos::create_signal(Option::<ServiceNameList>::None);
    let (selected_service_name_r, selected_service_name_w) =
        leptos::create_signal(Option::<String>::None);
    let (logs_r, logs_w) = leptos::create_signal(Option::<Vec<Log>>::None);
    let _api_service_names_request =
        leptos::create_local_resource(move || (), move |_| get_service_list(service_name_list_w));
    let _api_service_logs_request = leptos::create_local_resource(
        move || selected_service_name_r.get(),
        move |service_name| get_logs(logs_w, service_name),
    );
    let on_selected = move |ev: leptos::ev::Event| {
        let selected = leptos::event_target_value(&ev);
        log!("{:?}", selected);
        selected_service_name_w.set(Some(selected));
    };
    let logs_view = move || {
        let logs = logs_r.get();
        match logs {
            None => {
                view! {
                    <div style="padding: 20px; color: white">
                       <p>"Loading, maybe failed, check logs"</p>
                    </div>
                }
            }
            Some(logs) => {
                let logs_view = logs.iter().map(|l|{
                    let date = crate::printable_local_date_ms(l.timestamp);
                    let event_msg = format!("{date} - {}", l.value);
                    let color = match l.severity{
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
                    view!{
                        <>
                            <div style="width: 100%; background-color: rgba(255,255,255,0.05)">
                                <p class="trace-details__event" style={format!("color: {color}")}>{event_msg}</p>
                            </div>
                        </>
                    }
                }).collect_view();
                view! {
                    <div style="padding: 20px; color: white">
                       {logs_view}
                    </div>
                }
            }
        }
    };
    let view = move || match service_name_list_r.get() {
        None => {
            view! {
                <div style="padding: 20px; color: white">
                   <p>"Loading, maybe failed, check logs"</p>
                </div>
            }
        }
        Some(instance) if instance.is_empty() => {
            view! {
                <div style="padding: 20px; color: white">
                   <p>"Loaded, but no instances running"</p>
                </div>
            }
        }

        Some(instances) => {
            selected_service_name_w.set(instances.get(0).cloned());
            let options = instances
                .iter()
                .map(|service| {
                    view! {
                        <option value={service}>{service}</option>
                    }
                })
                .collect_view();
            view! {
                <div style="padding: 20px; color: white">
                    <select on:change=on_selected name="service-names" id="service-names">
                        {options}
                    </select>
                </div>
            }
        }
    };
    view! {
        <>
        {view}
        {logs_view}
        </>
    }
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

async fn get_service_list(w: WriteSignal<Option<ServiceNameList>>) {
    log!("Sending get_service_list request");
    let service_list: ServiceNameList = gloo_net::http::Request::get(&format!(
        "{}/api/logs/service_names",
        API_SERVER_URL_NO_TRAILING_SLASH
    ))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    log!("Got logs service list back: {:?}", service_list);
    w.set(Some(service_list));
}

async fn get_logs(w: WriteSignal<Option<Vec<Log>>>, service_name: Option<String>) {
    let service_name = match service_name {
        None => {
            log!("Empty service name");
            return;
        }
        Some(service_name) => service_name,
    };
    log!("Service name: {}", service_name);
    let query_params = ServiceLogRequest {
        service_name,
        start_time: 0,
    };
    let logs: Vec<Log> =
        gloo_net::http::Request::get(&format!("{}/api/logs", API_SERVER_URL_NO_TRAILING_SLASH))
            .query([
                ("service_name", query_params.service_name),
                ("start_time", query_params.start_time.to_string()),
            ])
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
    log!("Got logs back: {logs:#?}");
    w.set(Some(logs));
}
