use crate::API_SERVER_URL_NO_TRAILING_SLASH;
use api_structs::ui::live_services::LiveInstances;
use api_structs::ui::NewFiltersRequest;
use charming::component::{Axis, Legend, Title};
use charming::datatype::{CompositeValue, NumericValue};
use charming::element::{AxisType, Label, NameLocation, TextStyle, Tooltip, Trigger, TriggerOn};
use charming::series::{Line, Scatter};
use charming::{Chart, WasmRenderer};
use leptos::html::{Div, Input};
use leptos::logging::log;
use leptos::{
    component, create_action, create_node_ref, view, IntoView, NodeRef, Signal, SignalGet,
    SignalSet, WriteSignal,
};

#[derive(Debug, Clone)]
pub struct GraphSeries {
    name: String,
    x_values: Vec<i32>,
    y_values: Vec<i32>,
}
#[derive(Debug, Clone)]
pub struct GraphData {
    dom_id_to_render_to: String,
    y_name: String,
    x_name: String,
    series: Vec<GraphSeries>,
}

#[component]
pub fn Services(root_path: String) -> impl IntoView {
    let (trace_spans_r, trace_spans_w) = leptos::create_signal(Option::<LiveInstances>::None);
    let (instance_idx_r, instance_idx_w) = leptos::create_signal(Option::<usize>::None);
    let _api_request_sender =
        leptos::create_local_resource(move || (), move |_| get_summary(trace_spans_w));
    let change_filters_action =
        create_action(move |new_filters: &NewFiltersRequest| update_filter(new_filters.clone()));
    let create_chart_action = create_action(move |graph_data: &GraphData| {
        let el_id = graph_data.dom_id_to_render_to.clone();
        let graph_data = graph_data.clone();
        async move {
            let mut chart = Chart::new()
                .x_axis(
                    Axis::new()
                        .type_(AxisType::Value)
                        .name_location(NameLocation::Middle)
                        .name_text_style(TextStyle::new().font_size(18.))
                        .name(&graph_data.x_name)
                        .name_gap(20.),
                )
                .y_axis(
                    Axis::new()
                        .type_(AxisType::Value)
                        .name(&graph_data.y_name)
                        .name_text_style(TextStyle::new().font_size(18.))
                        .name_gap(30.)
                        .name_location(NameLocation::Middle),
                )
                .legend(
                    Legend::new()
                        .data(
                            graph_data
                                .series
                                .iter()
                                .map(|s| s.name.clone())
                                .collect::<Vec<String>>(),
                        )
                        .show(true),
                )
                .tooltip(
                    Tooltip::new()
                        .trigger(Trigger::Axis)
                        .trigger_on(TriggerOn::MousemoveAndClick),
                );
            for series in graph_data.series {
                chart = chart.series(
                    Line::new()
                        .data(
                            series
                                .x_values
                                .iter()
                                .zip(series.y_values.iter())
                                .map(|(a, b)| {
                                    CompositeValue::Array(vec![
                                        CompositeValue::Number(NumericValue::Integer(*a as i64)),
                                        CompositeValue::Number(NumericValue::Integer(*b as i64)),
                                    ])
                                })
                                .collect::<Vec<CompositeValue>>(),
                        )
                        .name(&series.name),
                );
            }

            let renderer = WasmRenderer::new(500, 300);
            let chart_instance = renderer.render(el_id.to_string().as_str(), &chart).unwrap();
            WasmRenderer::on_event(&chart_instance, "click", move |c| {
                instance_idx_w.set(Some(c.data_index));
                log!("{:#?}", c)
            });
            ()
        }
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
                    let spe_per_minute_limit =
                        stats.sampler_limits.trace_spe_per_minute_per_trace_limit;

                    let input_element: NodeRef<Input> = create_node_ref();

                    let increment = move |_| {
                        change_filters_action.dispatch(NewFiltersRequest {
                            instance_id: instance.service_id,
                            filters: input_element.get().unwrap().value(),
                        });
                    };
                    let active_traces_graph = NodeRef::<Div>::new();
                    let active_traces_graph_id = "active_traces".to_string();
                    active_traces_graph.on_load({
                        let active_traces_graph_id = active_traces_graph_id.clone();
                        move |e| {
                            let active_traces = GraphData {
                                dom_id_to_render_to: active_traces_graph_id.clone(),
                                y_name: "Active Traces".to_string(),
                                x_name: "minutes ago".to_string(),
                                series: vec![
                                    GraphSeries {
                                        name: "instance_1".to_string(),
                                        x_values: vec![30, 25, 20, 15, 10, 5, 0],
                                        y_values: vec![1, 2, 1, 3, 5, 1, 7],
                                    },
                                    GraphSeries {
                                        name: "instance_2".to_string(),
                                        x_values: vec![30, 21, 20, 15, 10, 5, 0],
                                        y_values: vec![3, 1, 5, 7, 6, 2, 1],
                                    },
                                ],
                            };
                            create_chart_action.dispatch(active_traces);
                        }
                    });
                    let spe_per_min_graph = NodeRef::<Div>::new();
                    let spe_per_min_graph_id = "spe_per_min".to_string();
                    spe_per_min_graph.on_load({
                        let spe_per_min_graph_id = spe_per_min_graph_id.clone();
                        move |e| {
                            let spe_per_min = GraphData {
                                dom_id_to_render_to: spe_per_min_graph_id.clone(),
                                y_name: "SpE/min".to_string(),
                                x_name: "minutes ago".to_string(),
                                series: vec![
                                    GraphSeries {
                                        name: "instance_1".to_string(),
                                        x_values: vec![30, 25, 20, 15, 10, 5, 0],
                                        y_values: vec![1, 2, 1, 3, 5, 1, 7],
                                    },
                                    GraphSeries {
                                        name: "instance_2".to_string(),
                                        x_values: vec![30, 21, 20, 15, 10, 5, 0],
                                        y_values: vec![3, 1, 5, 7, 6, 2, 1],
                                    },
                                ],
                            };
                            create_chart_action.dispatch(spe_per_min);
                        }
                    });

                    let logs_per_min_graph = NodeRef::<Div>::new();
                    let logs_per_min_graph_id = "logs_per_min".to_string();
                    logs_per_min_graph.on_load({
                        let logs_per_min_graph_id = logs_per_min_graph_id.clone();
                        move |e| {
                            let logs_per_min = GraphData {
                                dom_id_to_render_to: logs_per_min_graph_id.clone(),
                                y_name: "Logs/min".to_string(),
                                x_name: "minutes ago".to_string(),
                                series: vec![
                                    GraphSeries {
                                        name: "instance_1".to_string(),
                                        x_values: vec![30, 25, 20, 15, 10, 5, 0],
                                        y_values: vec![1, 2, 1, 3, 5, 1, 7],
                                    },
                                    GraphSeries {
                                        name: "instance_2".to_string(),
                                        x_values: vec![30, 21, 20, 15, 10, 5, 0],
                                        y_values: vec![3, 1, 5, 7, 6, 2, 1],
                                    },
                                ],
                            };
                            create_chart_action.dispatch(logs_per_min);
                        }
                    });

                    let export_buffer_graph = NodeRef::<Div>::new();
                    let export_buffer_graph_id = "export_buffer".to_string();
                    export_buffer_graph.on_load({
                        let export_buffer_graph_id = export_buffer_graph_id.clone();
                        move |e| {
                            let export_buffer = GraphData {
                                dom_id_to_render_to: export_buffer_graph_id.clone(),
                                y_name: "Export Buffer".to_string(),
                                x_name: "minutes ago".to_string(),
                                series: vec![
                                    GraphSeries {
                                        name: "instance_1".to_string(),
                                        x_values: vec![30, 25, 20, 15, 10, 5, 0],
                                        y_values: vec![1, 2, 1, 3, 5, 1, 7],
                                    },
                                    GraphSeries {
                                        name: "instance_2".to_string(),
                                        x_values: vec![30, 21, 20, 15, 10, 5, 0],
                                        y_values: vec![3, 1, 5, 7, 6, 2, 1],
                                    },
                                ],
                            };
                            create_chart_action.dispatch(export_buffer);
                        }
                    });

                    let logs_dropped_per_min_graph = NodeRef::<Div>::new();
                    let logs_dropped_per_min_graph_id = "logs_dropped_per_min".to_string();
                    logs_dropped_per_min_graph.on_load({
                        let logs_dropper_per_min_graph_id = logs_dropped_per_min_graph_id.clone();
                        move |e| {
                            let logs_dropped_per_min = GraphData {
                                dom_id_to_render_to: logs_dropper_per_min_graph_id.clone(),
                                y_name: "Logs Dropped/min".to_string(),
                                x_name: "minutes ago".to_string(),
                                series: vec![
                                    GraphSeries {
                                        name: "instance_1".to_string(),
                                        x_values: vec![30, 25, 20, 15, 10, 5, 0],
                                        y_values: vec![1, 2, 1, 3, 5, 1, 7],
                                    },
                                    GraphSeries {
                                        name: "instance_2".to_string(),
                                        x_values: vec![30, 21, 20, 15, 10, 5, 0],
                                        y_values: vec![3, 1, 5, 7, 6, 2, 1],
                                    },
                                ],
                            };
                            create_chart_action.dispatch(logs_dropped_per_min);
                        }
                    });

                    let dropped_traces_per_min_graph = NodeRef::<Div>::new();
                    let dropped_traces_per_min_graph_id = "dropped_traces_per_min".to_string();
                    dropped_traces_per_min_graph.on_load({
                        let dropped_traces_per_min_graph_id =
                            dropped_traces_per_min_graph_id.clone();
                        move |e| {
                            let dropped_traces_per_min = GraphData {
                                dom_id_to_render_to: dropped_traces_per_min_graph_id.clone(),
                                y_name: "Dropped Traces/min".to_string(),
                                x_name: "minutes ago".to_string(),
                                series: vec![
                                    GraphSeries {
                                        name: "instance_1".to_string(),
                                        x_values: vec![30, 25, 20, 15, 10, 5, 0],
                                        y_values: vec![1, 2, 1, 3, 5, 1, 7],
                                    },
                                    GraphSeries {
                                        name: "instance_2".to_string(),
                                        x_values: vec![30, 21, 20, 15, 10, 5, 0],
                                        y_values: vec![3, 1, 5, 7, 6, 2, 1],
                                    },
                                ],
                            };
                            create_chart_action.dispatch(dropped_traces_per_min);
                        }
                    });

                    let events_kb_per_min_graph = NodeRef::<Div>::new();
                    let events_kb_per_min_graph_id = "events_kb_per_min_graph".to_string();
                    events_kb_per_min_graph.on_load({
                        let events_kb_per_min_graph_id = events_kb_per_min_graph_id.clone();
                        move |e| {
                            let events_kb_per_min = GraphData {
                                dom_id_to_render_to: events_kb_per_min_graph_id.clone(),
                                y_name: "Events kb/min".to_string(),
                                x_name: "minutes ago".to_string(),
                                series: vec![
                                    GraphSeries {
                                        name: "instance_1".to_string(),
                                        x_values: vec![30, 25, 20, 15, 10, 5, 0],
                                        y_values: vec![1, 2, 1, 3, 5, 1, 7],
                                    },
                                    GraphSeries {
                                        name: "instance_2".to_string(),
                                        x_values: vec![30, 21, 20, 15, 10, 5, 0],
                                        y_values: vec![3, 1, 5, 7, 6, 2, 1],
                                    },
                                ],
                            };
                            create_chart_action.dispatch(events_kb_per_min);
                        }
                    });

                    let mut html_trace_stats = vec![];
                    for (trace_name, trace_stats) in stats.per_minute_trace_stats {
                        let dropped_traces_per_minute =
                            trace_stats.traces_dropped_by_sampling_per_minute;
                        let spe_usage_per_minute = trace_stats.spe_usage_per_minute;
                        html_trace_stats.push(view!{
                            <tr class={"row-container"}>
                                <td class="trace-table__cell">{{trace_name}}</td>
                                <td class="trace-table__cell">{format!("{}", spe_usage_per_minute)}</td>
                                <td class="trace-table__cell">{format!("{}", dropped_traces_per_minute)}</td>
                            </tr>
                        });
                    }
                    instances.push(view! {
                    <>
                        <div style="display: flex; gap: 20px; justify-content: center; align-items: center">
                            <p style="text-align: center">{format!("Instance {} Last seen: {} s ago", instance.service_id, secs_since_seen)}</p>
                            <div style="display: flex; justify-content: center">
                                <label style="align-self: center" for="filters">"RUST_LOG Filters: "</label>
                                <input type="text" id="filters" name="filters" node_ref=input_element value={&instance.filters} size="70" />
                                <button style="margin-left: 5px;" on:click=increment>"Apply"</button>
                            </div>
                        </div>
                        <div style="display: flex; gap: 20px; justify-content: center; align-items: center">
                            <p style="text-align: center; margin: 5px 0 5px 0">{format!("Instance {} Last seen: {} s ago", instance.service_id, secs_since_seen)}</p>
                            <div style="display: flex; justify-content: center">
                                <label style="align-self: center" for="filters">"RUST_LOG Filters: "</label>
                                <input type="text" id="filters" name="filters" node_ref=input_element value={&instance.filters} size="70" />
                                <button style="margin-left: 5px;" on:click=increment>"Apply"</button>
                            </div>
                        </div>
                        <div style="display: flex; flex-wrap: wrap; justify-content: center; margin: 10px 0 10px 0">
                            <div _ref=active_traces_graph id=active_traces_graph_id.clone()></div>
                            <div _ref=spe_per_min_graph id=spe_per_min_graph_id.clone()></div>
                            <div _ref=logs_per_min_graph id=logs_per_min_graph_id.clone()></div>
                            <div _ref=export_buffer_graph id=export_buffer_graph_id.clone()></div>
                            <div _ref=logs_dropped_per_min_graph id=logs_dropped_per_min_graph_id.clone()></div>
                            <div _ref=dropped_traces_per_min_graph id=dropped_traces_per_min_graph_id.clone()></div>
                            <div _ref=events_kb_per_min_graph id=events_kb_per_min_graph_id.clone()></div>
                        </div>
                        <p style="text-align: center">{"Graph Alerts:"}</p>
                        <div style="display: flex; flex-wrap: wrap; gap: 20px; justify-content: center">
                            <div style="">
                                <label style="align-self: center" for="filters">"Min Instance Count: "</label>
                                <input type="text"  name="filters" size="5" />
                                <button style="margin-left: 5px; margin-bottom: 10px">"Apply"</button>
                            </div>
                            <div style="">
                                <label style="align-self: center" for="filters">"Active Traces: "</label>
                                <input type="text"  name="filters" size="5" />
                                <button style="margin-left: 5px; margin-bottom: 10px">"Apply"</button>
                            </div>
                            <div style="">
                                <label style="align-self: center" for="filters">"SpE/min: "</label>
                                <input type="text"  name="filters" size="5" />
                                <button style="margin-left: 5px; margin-bottom: 10px">"Apply"</button>
                            </div>
                            <div style="">
                                <label style="align-self: center" for="filters">"Log/min: "</label>
                                <input type="text"  name="filters" size="5" />
                                <button style="margin-left: 5px; margin-bottom: 10px">"Apply"</button>
                            </div>
                            <div style="">
                                <label style="align-self: center" for="filters">"Export Buffer: "</label>
                                <input type="text"  name="filters" size="5" />
                                <button style="margin-left: 5px; margin-bottom: 10px">"Apply"</button>
                            </div>
                            <div style="">
                                <label style="align-self: center" for="filters">"Logs Dropped/min: "</label>
                                <input type="text" name="filters" size="5" />
                                <button style="margin-left: 5px; margin-bottom: 10px">"Apply"</button>
                            </div>
                            <div style="">
                                <label style="align-self: center" for="filters">"Dropped Traces/min: "</label>
                                <input type="text" name="filters" size="5" />
                                <button style="margin-left: 5px; margin-bottom: 10px">"Apply"</button>
                            </div>
                            <div style="">
                                <label style="align-self: center" for="filters">"Events kb/min: "</label>
                                <input type="text" name="filters" size="5" />
                                <button style="margin-left: 5px; margin-bottom: 10px">"Apply"</button>
                            </div>

                        </div>
                        <p style="text-align: center">{"Per Trace Global Alerts:"}</p>
                        <div style="display: flex; flex-wrap: wrap; gap: 20px; justify-content: center">
                            <div style="">
                                <label style="align-self: center" for="filters">"Warning %"</label>
                                <input type="text" name="filters" size="5" />
                                <button style="margin-left: 5px; margin-bottom: 10px">"Apply"</button>
                            </div>
                            <div style="">
                                <label style="align-self: center" for="filters">"Error %"</label>
                                <input type="text" name="filters" size="5" />
                                <button style="margin-left: 5px; margin-bottom: 10px">"Apply"</button>
                            </div>
                            <div style="">
                                <label style="align-self: center" for="filters">"Duration (ms): "</label>
                                <input type="text" name="filters" size="5" />
                                <button style="margin-left: 5px; margin-bottom: 10px">"Apply"</button>
                            </div>
                        </div>

                        <p style="text-align: center">{"Per Trace Alert overwrites:"}</p>
                        <div style="display: flex; flex-wrap: wrap; gap: 20px; justify-content: center">
                            <div style="">
                                <label style="align-self: center" for="trace_name">"Trace Name:"</label>
                                <input style="margin-right: 7px" type="text" name="trace_name" size="15" />
                                <label style="align-self: center" for="warning_percentage">"Warning %:"</label>
                                <input style="margin-right: 7px" type="text" name="warning_percentage" size="3" />
                                <label style="align-self: center" for="error_percentage">"Error %:"</label>
                                <input style="margin-right: 7px" type="text" name="error_percentage" size="3" />
                                <label style="align-self: center" for="trace_duration">"Duration (ms):"</label>
                                <input style="margin-right: 7px" type="text" name="trace_duration" size="3" />
                                <button style="margin-left: 5px; margin-bottom: 10px">"Apply"</button>
                            </div>
                        </div>

                        <table class="trace-table">

                            <tr class="row-container">
                                <th style="text-align: center" colspan="4" class="trace-table__cell">
                                    <a>"Trace Alert Overrides"</a>
                                </th>
                            </tr>
                            <tr class="row-container">
                                <th class="trace-table__cell">
                                    <a>"Trace Name"</a>
                                </th>
                                <th class="trace-table__cell">
                                    <a>"Allowed Warning %"</a>
                                </th>
                                <th class="trace-table__cell">
                                    <a>"Allowed Error %"</a>
                                </th>
                                <th class="trace-table__cell">
                                    <a>"Allowed Duration (ms)"</a>
                                </th>
                            </tr>
                        </table>
                        <p style="text-align: center">{format!("Active Traces {} min ago", 5)}</p>
                        <table class="trace-table">
                            <tr class="row-container">
                                <th class="trace-table__cell">
                                    <a>"Trace Name"</a>
                                </th>
                                <th class="trace-table__cell">
                                    <a>"Instance Id"</a>
                                </th>
                                <th class="trace-table__cell">
                                    <a>{"Duration (ms)"}</a>
                                </th>
                                <th class="trace-table__cell">
                                    <a>"Total / Lost Spans"</a>
                                </th>
                                <th class="trace-table__cell">
                                    <a>"Total / Lost Events"</a>
                                </th>
                                <th class="trace-table__cell">
                                    <a>"Events kb"</a>
                                </th>
                                <th class="trace-table__cell">
                                    <a>"Warns"</a>
                                </th>
                                <th class="trace-table__cell">
                                    <a>"➔"</a>
                                </th>
                            </tr>
                            {html_trace_stats}
                        </table>
                    </>
                });
                }
                els.push(view! {
                    <>
                        <h2 style="text-align: center">{format!("Service: {service}")}</h2>
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
