use crate::dashboard::graph_creation::GraphData;
use crate::TrackedGlooError;
use api_structs::ui::series::AlertCheckResult;
use api_structs::Endpoint;
use leptos::prelude::*;
use std::collections::HashMap;
use tracing::info;

pub mod graph_creation;
#[component]
pub fn Dashboard() -> impl IntoView {
    let (service_data_r, service_data_w) = signal_local::<
        Option<Result<Vec<api_structs::ui::series::SeriesWithData>, TrackedGlooError>>,
    >(None);
    let _api_service_list_request_sender =
        LocalResource::new(move || get_and_write_get_series_data_result(service_data_w));
    view! {
         <div style="padding: 20px; color: white">
            <DashboardContainer service_signal=service_data_r />
        </div>
    }
}

#[component]
pub fn Series(mut series: api_structs::ui::series::SeriesWithData) -> impl IntoView {
    let mut graph_series: HashMap<String, graph_creation::GraphSeries> = HashMap::new();
    for data_point in series.data {
        // match (data_point.data, data_point.series_id) {
        //     (Some(data), Some(series_id)) => {
        //         let graph_series = graph_series
        //             .entry(series_id.clone())
        //             .or_insert(graph_creation::GraphSeries::new(series_id.clone()));
        //         graph_series.push_data(
        //             data_point.time.timestamp_nanos_opt().unwrap() as u64,
        //             data as f64,
        //         );
        //     }
        //     _ => {
        //         continue;
        //     }
        // }
    }
    let graph_series: Vec<graph_creation::GraphSeries> = graph_series.into_values().collect();
    let data = GraphData {
        dom_id_to_render_to: series.id.to_string(),
        y_name: series.name,
        x_name: "minutes ago".to_string(),
        series: graph_series,
        click_event_timestamp_receiver: None,
    };
    let action = graph_creation::create_create_chart_action();
    let (trace_warning_graph, trace_warning_graph_id) =
        graph_creation::create_dom_el_ref_and_graph_call_action(data, action);
    let max_missing_data_points = series
        .max_missing_data_points
        .map(|e| e.to_string())
        .unwrap_or_else(|| "<None>".to_string());
    let max_value_threshold_str = series
        .max_value_threshold
        .map(|e| e.to_string())
        .unwrap_or_else(|| "<None>".to_string());
    let min_value_threshold_str = series
        .min_value_threshold
        .map(|e| e.to_string())
        .unwrap_or_else(|| "<None>".to_string());
    let check_window = series.check_window;
    let mut alerts = vec![];
    series.alert_checks.sort_by_key(|e| e.checked_at);
    series.alert_checks.reverse();
    for single_alert_check in series.alert_checks {
        let date = crate::datetime::printable_local_date(
            single_alert_check.checked_at.timestamp_nanos_opt().unwrap() as u64,
        );
        let ok_or_alert = match single_alert_check.result {
            AlertCheckResult::Ok => "Ok".to_string(),
            AlertCheckResult::AlertMessage(alert) => alert,
        };
        let msg = format!("{date} - {ok_or_alert}");
        alerts.push(view! {
            <h4>{msg}</h4>
        });
    }
    view! {
        <div
        style="display: flex; flex-wrap: wrap; justify-content: center; margin: 5px 0 5px 0;\
                border: 1px solid; padding: 20px"
            >
            <div id="alerts" style="padding: 10px 35px; border: 1px solid">
                <h2>"Alert Config:"</h2>
                <h4>{format!("Check Window: {check_window}")}</h4>
                <h4>{format!("Max Missing Data Points: {max_missing_data_points}")}</h4>
                <h4>{format!("Max Value Threshold: {max_value_threshold_str}")}</h4>
                <h4>{format!("Min Value Threshold: {min_value_threshold_str}")}</h4>
            </div>
            <div node_ref=trace_warning_graph id=trace_warning_graph_id.clone()></div>
            <div id="alert-checks" style="padding: 10px 35px; border: 1px solid; width: 400px; height: 500px; overflow: scroll;">
                <h2>"Alert Check Results:"</h2>
                {alerts}
            </div>
        </div>
    }
}

#[component]
pub fn DashboardContainer(
    service_signal: ReadSignal<
        Option<Result<Vec<api_structs::ui::series::SeriesWithData>, TrackedGlooError>>,
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
            Ok(series_with_data) => {
                let all_series: Vec<View<_>> = series_with_data
                    .into_iter()
                    .map(|s| {
                        view! { <Series series=s />}
                    })
                    .collect();
                let va = view! {
                   {all_series}
                };
                va.into_any()
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

async fn get_and_write_get_series_data_result(
    w: WriteSignal<
        Option<Result<Vec<api_structs::ui::series::SeriesWithData>, TrackedGlooError>>,
        LocalStorage,
    >,
) {
    info!("Sending get series req");
    let res = get_services_impl().await;
    info!("Got series data back");
    w.set(Some(res));
}

async fn get_services_impl(
) -> Result<Vec<api_structs::ui::series::SeriesWithData>, TrackedGlooError> {
    let services = gloo_net::http::Request::get(&format!(
        "{}{}",
        crate::API_SERVER_URL_NO_TRAILING_SLASH,
        api_structs::ui::series::GetSeries::PATH
    ))
    .send()
    .await?
    .json()
    .await?;
    Ok(services)
}
