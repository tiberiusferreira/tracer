use crate::error::TrackedGlooError;
use crate::graph_creation::GraphSeries;
use api_structs::Endpoint;
use chrono::NaiveTime;
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
pub fn Services() -> impl IntoView {
    view! {
        <div id="service-root" style="min-height:90vh; display: grid; align-content: start; column-gap: 15px; padding: 7px; color: white">
            <GlobalSelector/>
            <ServiceSelector/>

            <div id="overall-view" style="margin-top: 20px; ">
                <Visualizations/>
                <div id="grid-and-filters" style="display: grid; grid-template-columns: 3fr 1fr; margin-top: 10px">
                    <div id="trace-grid"  style="resize: vertical; min-height: 150px; margin: 0 0 0 0; padding: 7px; border: 1px solid white; border-radius: 10px; overflow: scroll;">
                        <TraceGrid/>
                    </div>
                    <div id="filters">
                        <PathFilter/>
                        <MethodFilter/>
                        <SeverityFilter/>
                    </div>
                </div>
            </div>
        </div>
    }
}

#[component]
fn PathFilter() -> impl IntoView {
    view! {
        <div style="resize: vertical; height: 150px; margin: 0 0 0 5px; padding: 10px; border: 1px solid white; border-radius: 10px; overflow: scroll;">
            <input style="margin-bottom: 5px" type="text" id="service-name" placeholder="Path" name="service-name-selector" />
            <div>
                <input type="checkbox" style="display: inline" id="scales" name="scales" checked />
                <label for="scales">"/api/traces - 1.2K "</label>
                <span>" - "</span>
                <button class="button-as-text">"only"</button>
                <button class="button-as-text" style="margin-left: 3px">"except"</button>
            </div>
            <div>
                <input type="checkbox" style="display: inline" id="scales" name="scales" checked />
                <label for="scales">"/api/traces/update - 1.5K "</label>
                <span>" - "</span>
                <button class="button-as-text">"only"</button>
                <button class="button-as-text" style="margin-left: 3px">"except"</button>
            </div>
        </div>
    }
}

#[component]
fn MethodFilter() -> impl IntoView {
    view! {
        <div style="resize: vertical; height: 150px; margin: 0 0 0 5px; padding: 10px; border: 1px solid white; border-radius: 10px; overflow: scroll;">
            <input style="margin-bottom: 5px" type="text" id="service-name" placeholder="Method" name="service-name-selector" />
                <div>
                    <input type="checkbox" style="display: inline" id="scales" name="scales" checked />
                    <label for="scales">"GET - 2.2k "</label>
                    <span>" - "</span>
                    <button class="button-as-text">"only"</button>
                    <button class="button-as-text" style="margin-left: 3px">"except"</button>
                </div>
                <div>
                    <input type="checkbox" style="display: inline" id="scales" name="scales" checked />
                    <label for="scales">"POST - 1.5K "</label>
                    <span>" - "</span>
                    <button class="button-as-text">"only"</button>
                    <button class="button-as-text" style="margin-left: 3px">"except"</button>
                </div>
        </div>
    }
}

#[component]
fn SeverityFilter() -> impl IntoView {
    view! {
        <div style="resize: vertical; height: 150px; margin: 0 0 0 5px; padding: 10px; border: 1px solid white; border-radius: 10px; overflow: scroll;">
            <input style="margin-bottom: 5px" type="text" id="service-name" placeholder="Severity" name="service-name-selector" />
                <div>
                    <input type="checkbox" style="display: inline" id="scales" name="scales" checked />
                    <label for="scales">"info - 2.2k "</label>
                    <span>" - "</span>
                    <button class="button-as-text">"only"</button>
                    <button class="button-as-text" style="margin-left: 3px">"except"</button>
                </div>
                <div>
                    <input type="checkbox" style="display: inline" id="scales" name="scales" checked />
                    <label for="scales">"warn - 2.2k "</label>
                    <span>" - "</span>
                    <button class="button-as-text">"only"</button>
                    <button class="button-as-text" style="margin-left: 3px">"except"</button>
                </div>
                <div>
                    <input type="checkbox" style="display: inline" id="scales" name="scales" checked />
                    <label for="scales">"error - 1.5K "</label>
                    <span>" - "</span>
                    <button class="button-as-text">"only"</button>
                    <button class="button-as-text" style="margin-left: 3px">"except"</button>
                </div>
        </div>
    }
}

#[component]
fn TraceGrid() -> impl IntoView {
    view! {
        <div>
            <InstanceUpdateRow/>
            <InstanceUpdateRow/>
        </div>
    }
}

#[component]
fn Visualizations() -> impl IntoView {
    let mut x: Vec<String> = vec![];
    let mut y: Vec<f64> = vec![];
    let start = chrono::NaiveDate::from_ymd_opt(2025, 2, 17)
        .unwrap()
        .and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    let mut curr = start;
    for i in 0..50 {
        x.push(curr.format("%H:%M:%S").to_string());
        curr += chrono::Duration::minutes(5);
        if i % 2 == 0 {
            y.push(500.);
        } else {
            y.push(100.);
        }
    }
    let graph_series: Vec<GraphSeries> = vec![GraphSeries {
        name: "series 1".to_string(),
        x_values: x,
        y_values: y,
    }];
    let data = crate::graph_creation::GraphData {
        dom_id_to_render_to: "some".to_string(),
        y_name: "traces".to_string(),
        x_name: "minutes ago".to_string(),
        series: graph_series.clone(),
        click_event_timestamp_receiver: None,
    };
    let data2 = crate::graph_creation::GraphData {
        dom_id_to_render_to: "some2".to_string(),
        y_name: "traces".to_string(),
        x_name: "minutes ago".to_string(),
        series: graph_series.clone(),
        click_event_timestamp_receiver: None,
    };

    let data3 = crate::graph_creation::GraphData {
        dom_id_to_render_to: "some3".to_string(),
        y_name: "duration".to_string(),
        x_name: "minutes ago".to_string(),
        series: graph_series.clone(),
        click_event_timestamp_receiver: None,
    };

    let data4 = crate::graph_creation::GraphData {
        dom_id_to_render_to: "some4".to_string(),
        y_name: "duration".to_string(),
        x_name: "minutes ago".to_string(),
        series: graph_series,
        click_event_timestamp_receiver: None,
    };
    let action = crate::graph_creation::create_create_chart_action();
    let (trace_warning_graph, trace_warning_graph_id) =
        crate::graph_creation::create_dom_el_ref_and_graph_call_action(data, action);

    let action2 = crate::graph_creation::create_create_chart_action();
    let (trace_warning_graph2, trace_warning_graph_id2) =
        crate::graph_creation::create_dom_el_ref_and_graph_call_action(data2, action2);

    let action3 = crate::graph_creation::create_create_chart_action();
    let (trace_warning_graph3, trace_warning_graph_id3) =
        crate::graph_creation::create_dom_el_ref_and_graph_call_action(data3, action3);

    let action4 = crate::graph_creation::create_create_chart_action();
    let (trace_warning_graph4, trace_warning_graph_id4) =
        crate::graph_creation::create_dom_el_ref_and_graph_call_action(data4, action4);
    view! {
         <div id="visualizations" style="resize: vertical; height: 200px; overflow: scroll; padding: 7px; border: 1px solid white; border-radius: 10px;">
                    <h3 style="display: inline; margin: 0 3px 0 0">"Rolled Over "</h3>
                    <select style="margin: 0 5px 0 5px" id="time-range-selector">
                            <option value="60">"1 min"</option>
                            <option value="60">"5 min"</option>
                    </select>
                    <div id="charts" style="display: grid; grid-template-columns: 3fr 3fr;">

                        <div id="traces-graph">
                            <div style="margin-top: 10px">
                                <h3 style="display: inline; margin: 0">"Traces: "</h3>
                                <p style="display: inline; margin: 0 0 0 10px">"5123 total (5.21/s) - 13 warnings (2.12/s) - 5 errors (0.13/s)"</p>
                                <div style="margin-left: auto">
                                    <p style="display: inline; margin: 0">"Alert Threshold"</p>
                                    <input style="margin-left: 5px" type="text" size="4" placeholder="Min" />
                                    <input style="margin-left: 5px" type="text" size="4" placeholder="Max" />
                                    <p style="display: inline; margin: 0">" over "</p>
                                    <p style="display: inline; margin: 0"><b>"1 min"</b></p>
                                    <input style="margin-left: 5px" type="text" size="8" placeholder="alert name" />
                                    <button style="margin-left: 5px">"Create"</button>
                                </div>
                            </div>
                            <div style="margin-top: 10px">
                                <div style="height: 100px" node_ref=trace_warning_graph id=trace_warning_graph_id.clone()></div>
                            </div>
                        </div>

                         <div id="request-charts">
                            <div style="margin-top: 10px">
                                <h3 style="display: inline; margin: 0">"Requests: "</h3>
                                <p style="display: inline; margin: 0 0 0 10px">"9132 total - [200] 512 (1.35/s) - [400] 600 (2.12/s) - [500] 702 (4.13/s) - [others]  3 (0.1/s)"</p>
                                <div style="margin-left: auto">
                                    <p style="display: inline; margin: 0">"Alert Threshold"</p>
                                    <input style="margin-left: 5px" type="text" size="4" placeholder="Min" />
                                    <input style="margin-left: 5px" type="text" size="4" placeholder="Max" />
                                    <p style="display: inline; margin: 0">" over "</p>
                                    <p style="display: inline; margin: 0"><b>"1 min"</b></p>
                                    <input style="margin-left: 5px" type="text" size="8" placeholder="alert name" />
                                    <button style="margin-left: 5px">"Create"</button>
                                </div>
                            </div>
                            <div style="margin-top: 10px">
                                <div style="height: 100px" node_ref=trace_warning_graph2 id=trace_warning_graph_id2.clone()></div>
                            </div>
                        </div>

                         <div id="request-charts">
                            <div style="margin-top: 10px">
                                <h3 style="display: inline; margin: 0">"Total Size Bytes: "</h3>
                                <p style="display: inline; margin: 0 0 0 10px">"23MB total "</p>
                                <div style="margin-left: auto">
                                    <p style="display: inline; margin: 0">"Alert Threshold"</p>
                                    <input style="margin-left: 5px" type="text" size="4" placeholder="Min" />
                                    <input style="margin-left: 5px" type="text" size="4" placeholder="Max" />
                                    <p style="display: inline; margin: 0">" over "</p>
                                    <p style="display: inline; margin: 0"><b>"1 min"</b></p>
                                    <input style="margin-left: 5px" type="text" size="8" placeholder="alert name" />
                                    <button style="margin-left: 5px">"Create"</button>
                                </div>
                            </div>
                            <div style="margin-top: 10px">
                                <div style="height: 100px" node_ref=trace_warning_graph4 id=trace_warning_graph_id4.clone()></div>
                            </div>
                        </div>


                         <div id="request-charts">
                            <div style="margin-top: 10px">
                                <h3 style="display: inline; margin: 0">"Trace Duration: "</h3>
                                <p style="display: inline; margin: 0 0 0 10px">"Max 5.3s - Min 0.1s - Avg 1.2s "</p>
                                <p style="display: inline; margin: 0">"- Bar shows"</p>
                                <select style="margin: 0 5px 0 5px" id="time-range-selector">
                                    <option value="60">"Min"</option>
                                    <option value="60">"Max"</option>
                                    <option value="60">"Avg"</option>
                                    <option value="60">"P90"</option>
                                    <option value="60">"P99"</option>
                                </select>
                                <div style="margin-left: auto">
                                    <p style="display: inline; margin: 0">"Alert Threshold"</p>
                                    <input style="margin-left: 5px" type="text" size="4" placeholder="Min" />
                                    <input style="margin-left: 5px" type="text" size="4" placeholder="Max" />
                                    <p style="display: inline; margin: 0">" over "</p>
                                    <p style="display: inline; margin: 0"><b>"1 min"</b></p>
                                    <input style="margin-left: 5px" type="text" size="8" placeholder="alert name" />
                                    <button style="margin-left: 5px">"Create"</button>
                                </div>
                            </div>
                            <div style="margin-top: 10px">
                                <div style="height: 100px" node_ref=trace_warning_graph3 id=trace_warning_graph_id3.clone()></div>
                            </div>
                        </div>

                    </div>
                </div>
    }
}
#[component]
fn ServiceSelector() -> impl IntoView {
    view! {
        <div id="service-selector" style="background-color: #29290645; resize: vertical; margin-top: 20px; height: 150px; padding: 7px; border: 1px solid white; border-radius: 10px; overflow: scroll;" >
            <div style="margin: 0px 0 10px 0">
                <input type="text" id="service-name" list="service-list" placeholder="Filter services" name="service-name-selector" />
            </div>
            <div>
                <input type="checkbox" style="display: inline" id="scales" name="scales" checked />
                <span>"Tracer Backend - 1.2K"</span> <span>" - "</span> <input type="text" size="100" value="tracing=info,tracing::background::jobs=warn"  /> <button style="margin: 0px 0 0 5px" type="button">apply</button>
                <ul style="margin: 5px 0 0 0">
                    <li style="margin: 5px 0 0 0">
                        <input type="checkbox" style="display: inline" id="scales" name="scales" checked />
                        <span>"Instance 1 - 612"</span> <span>" - "</span> <a href="https://www.w3schools.com">CPU Profile</a>
                    </li>
                    <li style="margin: 5px 0 0 0">
                        <input type="checkbox" style="display: inline" id="scales" name="scales" checked />
                        <span>"Instance 2 - 632"</span> <span>" - "</span> <a href="https://www.w3schools.com">CPU Profile</a>
                    </li>
                </ul>
            </div>
            <div>
                <input type="checkbox" style="display: inline" id="scales" name="scales" checked />
                <span>"Tracer UI - 1.2K"</span>
                <ul style="margin: 5px 0 0 0">
                    <li style="margin: 5px 0 0 0">
                        <input type="checkbox" style="display: inline" id="scales" name="scales" checked />
                        <span>"Instance 1 - 612"</span> <span>" - "</span> <a href="https://www.w3schools.com">CPU Profile</a>
                    </li>
                    <li style="margin: 5px 0 0 0">
                        <input type="checkbox" style="display: inline" id="scales" name="scales" checked />
                        <span>"Instance 2 - 632"</span> <span>" - "</span> <a href="https://www.w3schools.com">CPU Profile</a>
                    </li>
                </ul>
            </div>
        </div>
    }
}
#[component]
fn GlobalSelector() -> impl IntoView {
    view! {
        <div id="global-values" style="background-color: #29290645; padding: 7px; border: 1px solid white; border-radius: 10px" >
                <div id="time-selector">
                    <div style="margin-top: 0px">
                        <h3 style="display: inline;">"Preset: "</h3>
                        <select id="preset-selector">
                            <option value="15">
                                "
                                <none>
                                "
                            </option>
                            <option value="15">"All Traces"</option>
                            <option value="60">"Warnings"</option>
                            <option value="360">"Errors"</option>
                        </select>
                        <button style="margin-left: 5px">"Delete"</button>
                        <p style="display: inline; margin: 0 0 0 5px">"Save as"</p>
                        <input style="margin-left: 5px" type="text" size="6" placeholder="name" />
                        <button style="margin-left: 5px">"Save"</button>
                    </div>
                    <select style="margin: 5px 0 5px 0" id="time-range-selector">
                        <option value="5">"Past 5 minutes"</option>
                        <option value="15">"Past 15 minutes"</option>
                        <option value="60">"Past 1h"</option>
                        <option value="360">"Past 6h"</option>
                    </select>
                    <div>
                        <h3 style="display: inline">"Filters: "</h3>
                        <input style="margin-left: 5px" type="text" size="150" value="" readonly>< /input>
                    </div>
                </div>
            </div>
    }
}
#[component]
fn InstanceUpdateRow() -> impl IntoView {
    view! {
        <div style="margin: 5px 0 0 0; padding: 7px; border: 1px solid rgba(255, 255, 255, 0.4); border-radius: 10px; overflow: scroll;">
            <div>
                <p style="margin: 0">"Dev - Some Service - Instance Id: 132"</p>
            </div>
            <InstanceUpdateTrace/>
        </div>
    }
}

#[component]
fn InstanceUpdateTrace() -> impl IntoView {
    view! {
        <div>
            <table class="trace-table">
                <tr>
                    <th class="trace-table__cell">"name"</th>
                    <th class="trace-table__cell">"method"</th>
                    <th class="trace-table__cell">"path"</th>
                    <th class="trace-table__cell">"status"</th>
                    <th class="trace-table__cell">"duration"</th>
                    <th class="trace-table__cell">"size"</th>
                    <th class="trace-table__cell">"created at"</th>
                    <th class="trace-table__cell">"key"</th>
                    <th class="trace-table__cell">"value"</th>
                    <th class="trace-table__cell">"log"</th>
                    <th class="trace-table__cell">""</th>
                </tr>
                <tr>
                    <td colspan="11" style="background-color: rgba(255,255,255,0.10); border-radius: 10px;">
                        <div title="200ms" style="height: 10px; border-radius: 10px; background-color: white; width: 50px; margin-left: 50px">
                        </div>
                    </td>
                </tr>
                <tr>
                    <td class="trace-table__cell">"handler"</td>
                    <td class="trace-table__cell">"GET"</td>
                    <td class="trace-table__cell">"/api/path"</td>
                    <td class="trace-table__cell">"200"</td>
                    <td class="trace-table__cell">"1325ms"</td>
                    <td class="trace-table__cell">"1kb"</td>
                    <td class="trace-table__cell" title="dawdo">"2min ago"</td>
                    <td class="trace-table__cell">""</td>
                    <td class="trace-table__cell">""</td>
                    <td class="trace-table__cell">""</td>
                    <td class="trace-table__cell">"➔"</td>
                </tr>
                <tr>
                    <td colspan="11" style="background-color: rgba(255,255,255,0.10); border-radius: 10px;">
                        <div title="200ms" style="height: 10px; border-radius: 10px; background-color: white; width: 15px; margin-left: 100px">
                        </div>
                    </td>
                </tr>
                <tr>
                    <td class="trace-table__cell">"handler"</td>
                    <td class="trace-table__cell">"GET"</td>
                    <td class="trace-table__cell">"/api/path"</td>
                    <td class="trace-table__cell">"200"</td>
                    <td class="trace-table__cell">"1325ms"</td>
                    <td class="trace-table__cell">"1kb"</td>
                    <td class="trace-table__cell" title="dawdo">"2min ago"</td>
                    <td class="trace-table__cell">""</td>
                    <td class="trace-table__cell">""</td>
                    <td class="trace-table__cell">""</td>
                    <td class="trace-table__cell">"➔"</td>
                </tr>
                <tr>
                    <td colspan="11" style="background-color: rgba(255,255,255,0.10); border-radius: 10px;">
                        <div title="200ms" style="height: 10px; border-radius: 10px; background-color: white; width: 25px; margin-left: 100px">
                        </div>
                    </td>
                </tr>
                <tr>
                    <td class="trace-table__cell">"handler"</td>
                    <td class="trace-table__cell">"GET"</td>
                    <td class="trace-table__cell">"/api/path"</td>
                    <td class="trace-table__cell">"200"</td>
                    <td class="trace-table__cell">"1325ms"</td>
                    <td class="trace-table__cell">"1kb"</td>
                    <td class="trace-table__cell" title="dawdo">"2min ago"</td>
                    <td class="trace-table__cell">""</td>
                    <td class="trace-table__cell">""</td>
                    <td class="trace-table__cell">""</td>
                    <td class="trace-table__cell">"➔"</td>
                </tr>
            </table>
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
