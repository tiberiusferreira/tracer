use crate::error::TrackedGlooError;
use crate::graph_creation::{GraphData, GraphSeries};
use api_structs::Endpoint;
use api_structs::ui::service::{
    AttributeSummary, ExecutionHeader, ExecutionListFilters, ExecutionSummary, SummariesForGraph,
    SummaryFilters,
};
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, Timelike, Utc};
use leptos::prelude::*;
use leptos::tachys::prelude::*;
use leptos_router::NavigateOptions;
use std::cmp::min;
use std::collections::HashMap;
use std::fmt::Display;
use std::net::Shutdown::Write;
use std::ops::{Add, Sub};
use std::thread::current;
use tracing::info;

use crate::API_SERVER_URL_NO_TRAILING_SLASH;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SelectedAttribute {
    name: String,
    value: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ServiceState {
    time_range: TimeRange,
    current_selected_bucket: Option<DateTime<Utc>>,
    selected_attributes: Vec<SelectedAttribute>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TimeRange {
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
}

impl Default for ServiceState {
    fn default() -> Self {
        let end_time = Utc::now();
        let start_time = end_time - Duration::minutes(180);
        ServiceState {
            time_range: TimeRange {
                start_time,
                end_time,
            },
            current_selected_bucket: None,
            selected_attributes: vec![],
        }
    }
}

#[component]
pub fn Services() -> impl IntoView {
    let (query_params_r, query_params_w) =
        leptos_router::hooks::query_signal_with_options::<String>("state", {
            let mut default = NavigateOptions::default();
            default.scroll = false;
            default
        });
    let set_new_state: SignalSetter<ServiceState, LocalStorage> =
        SignalSetter::map(move |new: ServiceState| {
            query_params_w.set(Some(serde_json::to_string(&new).unwrap()));
        });
    if query_params_r.get_untracked().is_none() {
        set_new_state.set(ServiceState::default());
    }
    let state_r = Signal::derive_local(move || {
        query_params_r
            .get()
            .map(|state| serde_json::from_str(&state).unwrap())
            .unwrap_or(ServiceState::default())
    });
    let time_range_r = Signal::derive_local(move || state_r.with(|s| s.time_range.clone()));
    let (partial_name_r, partial_name_w) = signal_local("".to_string());
    let (partial_value_r, partial_value_w) = signal_local("".to_string());
    let current_time_bucket =
        Signal::derive_local(move || state_r.with(|s| s.current_selected_bucket.clone()));
    let set_time_range: SignalSetter<TimeRange, LocalStorage> =
        SignalSetter::map(move |val: TimeRange| {
            let mut new = state_r.get_untracked();
            new.time_range = val;
            query_params_w.set(Some(serde_json::to_string(&new).unwrap()));
        });
    let set_partial_attribute_name: SignalSetter<String, LocalStorage> =
        SignalSetter::map(move |val: String| {
            partial_name_w.set(val);
        });
    let set_partial_attribute_value: SignalSetter<String, LocalStorage> =
        SignalSetter::map(move |val: String| {
            partial_value_w.set(val);
        });

    let selected_attributes_r = Signal::derive_local(move || {
        let mut map: HashMap<String, Option<String>> = HashMap::new();
        let state = state_r.get();
        let attributes = state.selected_attributes;
        for a in attributes {
            map.insert(a.name, a.value);
        }
        map
    });
    let (service_data_r, service_data_w) =
        signal_local::<Option<Result<SummariesForGraph, TrackedGlooError>>>(None);
    let _api_service_list_request_sender = LocalResource::new(move || {
        let state = state_r.get();
        let attributes: HashMap<String, Option<String>> = selected_attributes_r.get();
        get_and_write_get_service_data_result(state.time_range, attributes, service_data_w)
    });

    let apply_filter = move |_| {
        let mut new_state = state_r.get();
        let new_attr_name = partial_name_r.get();
        if new_attr_name.is_empty() {
            return;
        }
        let mut new_attr_val = partial_value_r.get();
        let new_attr_val = if new_attr_val.is_empty() {
            None
        } else {
            Some(new_attr_val)
        };
        new_state
            .selected_attributes
            .retain(|a| a.name != new_attr_name);
        new_state.selected_attributes.push(SelectedAttribute {
            name: new_attr_name,
            value: new_attr_val,
        });
        set_new_state.set(new_state);
        set_partial_attribute_name.set("".to_string());
        set_partial_attribute_value.set("".to_string());
    };

    let selected_attribute_row_view = move || {
        let mut row = vec![];
        let attributes = selected_attributes_r.get();
        for (name, val) in attributes {
            let val = val.unwrap_or_default();
            let clear = {
                let name = name.clone();
                move |_| {
                    let mut state = state_r.get();
                    state.selected_attributes.retain(|a| a.name != name);
                    set_new_state.set(state);
                }
            };
            row.push(view! {
                <tr>
                    <td class="trace-table__cell">{name}</td>
                    <td class="trace-table__cell">{val}</td>
                    <td class="trace-table__cell"><button on:click=clear>"Clear"</button></td>
                </tr>
            });
        }
        row
    };

    let set_time_bucket = SignalSetter::map(move |time_bucket: Option<DateTime<Utc>>| {
        let mut new = state_r.get_untracked();
        new.current_selected_bucket = time_bucket;
        query_params_w.set(Some(serde_json::to_string(&new).unwrap()));
    });

    let attribute_name_selection_list = move || match service_data_r.get() {
        Some(Ok(data)) => {
            let mut data: Vec<(String, AttributeSummary)> = data.attributes.into_iter().collect();
            data.sort_by_key(|e| e.0.clone());
            let mut els = vec![];
            let partial_name = partial_name_r.get();
            for (k, v) in data {
                if !k.contains(&partial_name) {
                    continue;
                }
                let label = format!("{k} - {}", v.count);
                els.push(view! {
                    <div on:click=move |_| {
                            set_partial_attribute_name.set(k.clone());
                            set_partial_attribute_value.set("".to_string());
                        }
                        class="text-as-button" style="display: block; margin: 10px 5px 10px 5px; max-height:100px; white-space: pre; overflow: scroll; background-color: rgba(255,255,255,0.1)">{label}
                    </div>
                });
            }
            view! {
                <div style="max-height:100%; overflow: scroll" id="attr-name-checkbox-list">
                    {els}
                </div>
            }
            .into_any()
        }
        _ => view! {
            <div id="attr-name-checkbox-list">

            </div>
        }
        .into_any(),
    };

    let attribute_value_selection_view = move || {
        let attr_name = partial_name_r.get();
        if attr_name.is_empty() {
            return view! {
                <div id="attr-value-checkbox-list">
                </div>
            }
            .into_any();
        }
        match service_data_r.get() {
            Some(Ok(mut data)) => {
                let mut els = vec![];
                let mut values: Vec<(String, u32)> = data
                    .attributes
                    .remove(&attr_name)
                    .map(|v| v.values)
                    .unwrap_or_default()
                    .into_iter()
                    .collect();
                let partial_value = partial_value_r.get();
                values.sort_by_key(|e| e.0.clone());
                for (val, count) in &values {
                    if !val.contains(&partial_value) {
                        continue;
                    }
                    let val_2 = val.clone();
                    let label = format!("{val} - {count}");
                    els.push(view! {
                        <div on:click=move |_| {
                                set_partial_attribute_value.set(val_2.clone());
                            }
                            class="text-as-button" style="display: block; margin: 15px 5px 15px 5px; max-height:100px; white-space: pre; overflow: scroll; background-color: rgba(255,255,255,0.1)">{label}
                        </div>
                    });
                }
                view! {
                    <div id="attr-value-checkbox-list">
                        {els}
                    </div>
                }
                .into_any()
            }
            _ => view! {
                <div id="attr-value-checkbox-list">
                </div>
            }
            .into_any(),
        }
    };

    view! {
        <div id="service-root" style="min-height:90vh; display: grid; align-content: start; column-gap: 15px; margin: 30px; color: white">
            <GlobalSelector/>
            <div id="overall-view" style="margin-top: 20px; ">
                <Visualizations set_time_bucket=set_time_bucket service_data_r=service_data_r time_range_r=time_range_r set_time_range=set_time_range/>
                <ServiceSelector/>
                <div>
                    <div id="attributes-selector" style="background-color: #29290645; resize: vertical; margin-top: 20px; height: 350px; padding: 7px; border: 1px solid white; border-radius: 10px; overflow: scroll" >
                        <div style="display: flex; height:60%">
                            <div style="margin: 0px 10px 10px 0; border: solid 1px white; border-radius: 5px; padding: 5px; overflow: hidden; max-height:100%">
                                <input type="text" size="40" bind:value=(partial_name_r, partial_name_w) id="attr-name" list="attribute-name-list" placeholder="Attribute Name" name="attribute-selector" />
                                <div style="overflow: scroll">
                                    {attribute_name_selection_list}
                                </div>
                            </div>

                            <div style="margin: 0px 0px 10px 0; border: solid 1px white; border-radius: 5px; padding: 5px; overflow: hidden; flex-grow: 1">
                                <input type="text" size="110" bind:value=(partial_value_r, partial_value_w) id="attribute-value" list="attribute-val-list" placeholder="Attribute Value" name="attribute-val-selector" />
                                <button style="margin-left: 5px" on:click=apply_filter >"Apply"</button>
                                <div style="max-height:100%; overflow: scroll">
                                    {attribute_value_selection_view}
                                </div>
                            </div>
                        </div>
                        <div style="overflow: scroll; height:35%; margin: 0px 0px 5px 0px; border: white 1px solid; border-radius: 5px; padding: 3px;">
                            <table class="trace-table">
                                <tr>
                                    <th colspan="3" class="trace-table__cell">"Attribute Filters"</th>
                                </tr>
                                {selected_attribute_row_view}
                            </table>
                        </div>
                    </div>
                </div>
                <div id="grid-and-filters" style="display: grid; grid-template-columns: 4fr 0fr; margin: 0 0 100px 0">
                    <div id="trace-grid"  style="resize: vertical; min-height: 150px; margin: 0 0 0 0; padding: 7px; border: 1px solid white; border-radius: 10px; overflow: scroll;">
                        <div style="margin: 5px 0 0 0; padding: 7px; border: 1px solid rgba(255, 255, 255, 0.4); border-radius: 10px; overflow: scroll;">
                            <TracesGrid current_time_bucket=current_time_bucket selected_attributes_r=selected_attributes_r.into()/>
                        </div>
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
            </div>
            <div>
                <input type="checkbox" style="display: inline" id="scales" name="scales" checked />
                <label for="scales">"/api/traces/update - 1.5K "</label>
                <span>" - "</span>
                <button class="button-as-text">"only"</button>

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
                </div>
                <div>
                    <input type="checkbox" style="display: inline" id="scales" name="scales" checked />
                    <label for="scales">"POST - 1.5K "</label>
                    <span>" - "</span>
                    <button class="button-as-text">"only"</button>

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

                </div>
                <div>
                    <input type="checkbox" style="display: inline" id="scales" name="scales" checked />
                    <label for="scales">"warn - 2.2k "</label>
                    <span>" - "</span>
                    <button class="button-as-text">"only"</button>

                </div>
                <div>
                    <input type="checkbox" style="display: inline" id="scales" name="scales" checked />
                    <label for="scales">"error - 1.5K "</label>
                    <span>" - "</span>
                    <button class="button-as-text">"only"</button>

                </div>
        </div>
    }
}

fn service_graph(
    execution_summary: &SummariesForGraph,
    set_time_bucket: SignalSetter<Option<DateTime<Utc>>, LocalStorage>,
) -> AnyView {
    let action = crate::graph_creation::create_create_chart_action();
    let series = GraphSeries {
        name: "my series".to_string(),
        x_values: execution_summary
            .buckets
            .iter()
            .map(|f| {
                let f = f.with_timezone(&chrono::Local);
                f.format("%H:%M %d/%m/%Y").to_string()
            })
            .collect(),
        y_values: execution_summary.execution.values.clone(),
    };
    let total = execution_summary.execution.total;
    let warnings = execution_summary.execution.with_warning_count;
    let errors = execution_summary.execution.with_errors_count;
    let buckets = execution_summary.buckets.clone();
    let set_signal = SignalSetter::<_, LocalStorage>::map(move |index: u64| {
        let time: DateTime<Utc> = buckets[index as usize];
        set_time_bucket.set(Some(time));
    });

    let data = GraphData {
        dom_id_to_render_to: "exec_summary".to_string(),
        y_name: "TestY".to_string(),
        x_name: "TestX".to_string(),
        series: vec![series],
        click_event_timestamp_receiver: Some(set_signal),
    };
    let (trace_warning_graph, trace_warning_graph_id) =
        crate::graph_creation::create_dom_el_ref_and_graph_call_action(data, action);
    view! {
            <div id="traces-graph">
                <div style="margin-top: 10px">
                    <h3 style="display: inline; margin: 0">"Executions: "</h3>
                    <p style="display: inline; margin: 0 0 0 10px">{format!("{total} total - {warnings} warnings - {errors} errors")}</p>
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
    }.into_any()
}

fn requests_graph(
    execution_summary: &SummariesForGraph,
    set_time_bucket: SignalSetter<Option<DateTime<Utc>>, LocalStorage>,
) -> AnyView {
    let action = crate::graph_creation::create_create_chart_action();
    let series = GraphSeries {
        name: "requests".to_string(),
        x_values: execution_summary
            .buckets
            .iter()
            .map(|f| {
                let f = f.with_timezone(&chrono::Local);
                f.format("%H:%M").to_string()
            })
            .collect(),
        y_values: execution_summary.execution.values.clone(),
    };
    let total = execution_summary.requests.total;
    let with_200 = execution_summary.requests.with_200_status_count;
    let non_200 = execution_summary.requests.with_non_200_status_count;
    let buckets = execution_summary.buckets.clone();
    let set_signal = SignalSetter::<_, LocalStorage>::map(move |index: u64| {
        let time: DateTime<Utc> = buckets[index as usize];
        set_time_bucket.set(Some(time));
    });
    let data = GraphData {
        dom_id_to_render_to: "requests".to_string(),
        y_name: "TestY".to_string(),
        x_name: "TestX".to_string(),
        series: vec![series],
        click_event_timestamp_receiver: Some(set_signal),
    };
    let (trace_warning_graph, trace_warning_graph_id) =
        crate::graph_creation::create_dom_el_ref_and_graph_call_action(data, action);
    view! {
            <div id="request-charts">
                <div style="margin-top: 10px">
                    <h3 style="display: inline; margin: 0">"Requests: "</h3>
                    <p style="display: inline; margin: 0 0 0 10px">{format!("{total} total - [200] {with_200} - others {non_200}")}</p>
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
    }.into_any()
}

fn size_graph(
    execution_summary: &SummariesForGraph,
    set_time_bucket: SignalSetter<Option<DateTime<Utc>>, LocalStorage>,
) -> AnyView {
    let action = crate::graph_creation::create_create_chart_action();
    let series = GraphSeries {
        name: "size".to_string(),
        x_values: execution_summary
            .buckets
            .iter()
            .map(|f| {
                let f = f.with_timezone(&chrono::Local);
                f.format("%H:%M").to_string()
            })
            .collect(),
        y_values: execution_summary
            .size_bytes
            .values
            .clone()
            .into_iter()
            .map(|v| v / 1000_000.)
            .collect(),
    };
    let total_mb = execution_summary.size_bytes.total as f64 / 1000_000.0;
    let buckets = execution_summary.buckets.clone();
    let set_signal = SignalSetter::<_, LocalStorage>::map(move |index: u64| {
        let time: DateTime<Utc> = buckets[index as usize];
        set_time_bucket.set(Some(time));
    });
    let data = GraphData {
        dom_id_to_render_to: "size".to_string(),
        y_name: "TestY".to_string(),
        x_name: "TestX".to_string(),
        series: vec![series],
        click_event_timestamp_receiver: Some(set_signal),
    };
    let (trace_warning_graph, trace_warning_graph_id) =
        crate::graph_creation::create_dom_el_ref_and_graph_call_action(data, action);
    view! {
             <div id="size-charts">
                    <div style="margin-top: 10px">
                        <h3 style="display: inline; margin: 0">"Total Size Bytes: "</h3>
                        <p style="display: inline; margin: 0 0 0 10px">{format!("{total_mb:.2}MB total")}</p>
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

    }.into_any()
}

fn duration_graph(
    execution_summary: &SummariesForGraph,
    set_time_bucket: SignalSetter<Option<DateTime<Utc>>, LocalStorage>,
) -> AnyView {
    let action = crate::graph_creation::create_create_chart_action();
    let series = GraphSeries {
        name: "duration".to_string(),
        x_values: execution_summary
            .buckets
            .iter()
            .map(|f| {
                let f = f.with_timezone(&chrono::Local);
                f.format("%H:%M").to_string()
            })
            .collect(),
        y_values: execution_summary.duration.max_values.clone(),
    };
    let buckets = execution_summary.buckets.clone();
    let set_signal = SignalSetter::<_, LocalStorage>::map(move |index: u64| {
        let time: DateTime<Utc> = buckets[index as usize];
        set_time_bucket.set(Some(time));
    });
    let max_duration = execution_summary.duration.max_ms;
    let data = GraphData {
        dom_id_to_render_to: "duration".to_string(),
        y_name: "TestY".to_string(),
        x_name: "TestX".to_string(),
        series: vec![series],
        click_event_timestamp_receiver: Some(set_signal),
    };
    let (trace_warning_graph, trace_warning_graph_id) =
        crate::graph_creation::create_dom_el_ref_and_graph_call_action(data, action);
    view! {
              <div id="duration-charts">
                    <div style="margin-top: 10px">
                        <h3 style="display: inline; margin: 0">"Trace Duration: "</h3>
                        <p style="display: inline; margin: 0 0 0 10px">{format!("Max {max_duration:.0}ms")}</p>
                        <p style="display: inline; margin: 0">"- Bar shows"</p>
                        <select style="margin: 0 5px 0 5px" id="time-range-selector">
                            <option value="60">"Max"</option>
                            <option value="60">"Min"</option>
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
                        <div style="margin-top: 10px">
                                <div style="height: 100px" node_ref=trace_warning_graph id=trace_warning_graph_id.clone()></div>
                        </div>
                    </div>
             </div>

    }.into_any()
}

#[component]
fn Visualizations(
    service_data_r: ReadSignal<Option<Result<SummariesForGraph, TrackedGlooError>>, LocalStorage>,
    time_range_r: Signal<TimeRange, LocalStorage>,
    set_time_bucket: SignalSetter<Option<DateTime<Utc>>, LocalStorage>,
    set_time_range: SignalSetter<TimeRange, LocalStorage>,
) -> impl IntoView {
    let service_graph = move || match service_data_r.get() {
        None => {
            info!("empty");
            view! {
                <div>"empty"</div>
            }
            .into_any()
        }
        Some(value) => match value {
            Ok(data) => service_graph(&data, set_time_bucket),
            Err(err) => view! {
                <div><p>{format!("{err:#?}")}</p></div>
            }
            .into_any(),
        },
    };
    let reqs_graph = move || match service_data_r.get() {
        None => {
            info!("empty");
            view! {
                <div>"empty"</div>
            }
            .into_any()
        }
        Some(value) => match value {
            Ok(data) => crate::services::requests_graph(&data, set_time_bucket),
            Err(err) => view! {
                <div><p>{format!("{err:#?}")}</p></div>
            }
            .into_any(),
        },
    };

    let size_graph = move || match service_data_r.get() {
        None => {
            info!("empty");
            view! {
                <div>"empty"</div>
            }
            .into_any()
        }
        Some(value) => match value {
            Ok(data) => crate::services::size_graph(&data, set_time_bucket),
            Err(err) => view! {
                <div><p>{format!("{err:#?}")}</p></div>
            }
            .into_any(),
        },
    };

    let duration_graph = move || match service_data_r.get() {
        None => {
            info!("empty");
            view! {
                <div>"empty"</div>
            }
            .into_any()
        }
        Some(value) => match value {
            Ok(data) => crate::services::duration_graph(&data, set_time_bucket),
            Err(err) => view! {
                <div><p>{format!("{err:#?}")}</p></div>
            }
            .into_any(),
        },
    };
    let on_click_back = move |_| {
        let mut time_range = time_range_r.get_untracked();
        time_range.start_time = time_range.start_time.sub(Duration::minutes(60));
        time_range.end_time = time_range.end_time.sub(Duration::minutes(60));
        set_time_range.set(time_range);
    };
    let on_click_now = move |_| {
        let default = ServiceState::default();
        set_time_range.set(default.time_range);
    };
    let on_click_forward = move |_| {
        let mut time_range = time_range_r.get_untracked();
        time_range.start_time = time_range.start_time.add(Duration::minutes(60));
        time_range.end_time = time_range.end_time.add(Duration::minutes(60));
        set_time_range.set(time_range);
    };
    view! {
        <div id="visualizations" style="resize: vertical; height: 370px; overflow: scroll; padding: 7px; border: 1px solid white; border-radius: 10px;">
            <div style="display: flex; justify-content: center;">
                <button style="margin: 0 5px 0 0" on:click=on_click_back>"<- 1h"</button>
                <button style="margin: 0 5px 0 0" on:click=on_click_now>"now"</button>
                <button on:click=on_click_forward>"1h ->"</button>
            </div>
            <div id="charts" style="display: grid; grid-template-columns: 3fr 3fr;">
                {service_graph}
                {reqs_graph}
                {size_graph}
                {duration_graph}
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
            <div id="env-service-list">
                <ServiceInfo/>
                <ServiceInfo/>
            </div>
        </div>
    }
}

#[component]
fn ServiceInfo() -> impl IntoView {
    view! {
        <div id="single-env-info">
            <div id="env-info">
                <input type="checkbox" style="display: inline" id="scales" name="scales" checked />
                <span>"Dev - 2.2K"</span>
            </div>
            <ul style="margin: 5px 0 0 0">
                <li style="margin: 5px 0 0 0">
                    <div id="service-info">
                        <input type="checkbox" style="display: inline" id="scales" name="scales" checked />
                        <span>"Tracer Backend - 1.2K"</span>
                    </div>
                    <div id="service-instance-list">
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
                </li>
            </ul>
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
fn TracesGrid(
    current_time_bucket: Signal<Option<DateTime<Utc>>, LocalStorage>,
    selected_attributes_r: Signal<HashMap<String, Option<String>>, LocalStorage>,
) -> impl IntoView {
    let (service_data_r, service_data_w) =
        signal_local::<Option<Result<Vec<ExecutionHeader>, TrackedGlooError>>>(None);
    let _api_service_list_request_sender = LocalResource::new(move || {
        get_and_write_get_execution_headers_result(
            current_time_bucket.get().unwrap_or(Utc::now()),
            selected_attributes_r.get(),
            service_data_w,
        )
    });
    let view = move || match service_data_r.get() {
        None => view! {<p>"Loading..."</p>}.into_any(),
        Some(result) => match result {
            Ok(result) => {
                let mut rows = vec![];
                for (idx, r) in result.into_iter().enumerate() {
                    rows.push(grid_row(idx, r));
                }
                view! {
                    <div>
                        <table class="trace-table">
                            <tr>
                                <th class="trace-table__cell">"id"</th>
                                <th class="trace-table__cell">"Service"</th>
                                <th class="trace-table__cell">"Last Seen (minutes ago)"</th>
                                <th class="trace-table__cell">"Duration (ms)"</th>
                                <th class="trace-table__cell">"Size KB"</th>
                                <th class="trace-table__cell">"Status Code"</th>
                                <th class="trace-table__cell">"Path"</th>
                                <th class="trace-table__cell">"Method"</th>
                                <th class="trace-table__cell">"➔"</th>
                            </tr>
                            {rows}

                        </table>
                    </div>
                }
                .into_any()
            }
            Err(err) => view! { <div><p>{format!("{err:#?}")}</p></div>}.into_any(),
        },
    };
    view
}

fn grid_row(idx: usize, header: ExecutionHeader) -> impl IntoView {
    // http://127.0.0.1:8081/execution-details/45f2140c-427d-11f0-a284-a717315ab9a0
    let url = format!(
        "{API_SERVER_URL_NO_TRAILING_SLASH}/execution-details/{}",
        header.external_id.to_string()
    );
    let background = if idx % 2 == 0 {
        "background-color: #29290645;"
    } else {
        "background-color: black;"
    };
    view! {
        <tr>
            <td class="trace-table__cell" style={background}>{header.external_id.to_string()}</td>
            <td class="trace-table__cell" style={background}>{header.service_name}</td>
            <td class="trace-table__cell" style={background}>{(Utc::now() - header.started_at).num_minutes()}</td>
            <td class="trace-table__cell" style={background}>{header.duration_ms}</td>
            <td class="trace-table__cell" style={background}>{header.size_bytes/1000}</td>
            <td class="trace-table__cell" style={background}>{header.status_code}</td>
            <td class="trace-table__cell" style={background}>{header.path}</td>
            <td class="trace-table__cell" style={background}>{header.method}</td>
            <td class="trace-table__cell" style={background}>
                <a style="text-decoration: none" href={url}>"➔"</a>
            </td>
        </tr>
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

async fn get_and_write_get_execution_headers_result(
    datetime: chrono::DateTime<Utc>,
    attributes: HashMap<String, Option<String>>,
    w: WriteSignal<
        Option<Result<Vec<api_structs::ui::service::ExecutionHeader>, TrackedGlooError>>,
        LocalStorage,
    >,
) {
    let res = get_executions_headers_impl(datetime, attributes).await;
    w.set(Some(res));
}

async fn get_and_write_get_service_data_result(
    time_range: TimeRange,
    attributes: HashMap<String, Option<String>>,
    w: WriteSignal<Option<Result<SummariesForGraph, TrackedGlooError>>, LocalStorage>,
) {
    let res = get_services_impl(time_range, attributes).await;
    w.set(Some(res));
}

async fn get_services_impl(
    time_range: TimeRange,
    attributes: HashMap<String, Option<String>>,
) -> Result<SummariesForGraph, TrackedGlooError> {
    let services = gloo_net::http::Request::post(&format!(
        "{}{}",
        API_SERVER_URL_NO_TRAILING_SLASH, "/api/ui/service/data"
    ))
    .json(&SummaryFilters {
        start_date: time_range.start_time,
        end_date: time_range.end_time,
        attributes,
    })
    .unwrap()
    .send()
    .await?
    .json()
    .await?;
    Ok(services)
}

async fn get_executions_headers_impl(
    datetime: DateTime<Utc>,
    attributes: HashMap<String, Option<String>>,
) -> Result<Vec<ExecutionHeader>, TrackedGlooError> {
    let services = gloo_net::http::Request::post(&format!(
        "{}{}",
        API_SERVER_URL_NO_TRAILING_SLASH, "/api/ui/service/execution_list"
    ))
    .json(&ExecutionListFilters {
        bucket: datetime,
        attributes,
    })
    .unwrap()
    .send()
    .await?
    .json()
    .await?;
    Ok(services)
}
