use crate::{secs_since, API_SERVER_URL_NO_TRAILING_SLASH};
use api_structs::time_conversion::now_nanos_u64;
use api_structs::ui::live_services::LiveInstances;
use api_structs::ui::service_health::{Instance, ServiceData, TraceHeader};
use api_structs::ui::NewFiltersRequest;
use base64::Engine;
use charming::component::{Axis, DataZoom, DataZoomType, Legend, LegendItem, LegendType, Title};
use charming::datatype::{CompositeValue, NumericValue};
use charming::element::{
    AxisPointer, AxisPointerAxis, AxisType, Label, NameLocation, TextStyle, Tooltip, Trigger,
    TriggerOn,
};
use charming::series::{Line, Scatter};
use charming::{Chart, WasmRenderer};
use chrono::Duration;
use js_sys::encode_uri_component;
use leptos::html::{Div, Input};
use leptos::logging::log;
use leptos::{
    component, create_action, create_node_ref, view, Action, IntoView, NodeRef, ReadSignal, Signal,
    SignalGet, SignalSet, SignalSetUntracked, WriteSignal,
};
use std::collections::HashMap;
use std::rc::Rc;
use web_sys::MouseEvent;

#[derive(Debug, Clone)]
pub struct GraphSeries {
    name: String,
    original_x_values: Vec<u64>,
    x_values: Vec<f64>,
    y_values: Vec<f64>,
}
impl GraphSeries {
    pub fn new(name: String) -> Self {
        Self {
            name,
            original_x_values: vec![],
            x_values: vec![],
            y_values: vec![],
        }
    }
    pub fn push_data(&mut self, timestamp: u64, data: f64) {
        self.original_x_values.push(timestamp);
        let minutes_since = secs_since(timestamp) as f64 / 60.;
        self.x_values.push(minutes_since);
        self.y_values.push(data);
    }
}

#[derive(Debug, Clone)]
pub struct GraphData {
    dom_id_to_render_to: String,
    y_name: String,
    x_name: String,
    series: Vec<GraphSeries>,
    click_event_timestamp_receiver: Option<WriteSignal<Option<u64>>>,
}

fn create_chart_action() -> Action<GraphData, ()> {
    create_action(move |graph_data: &GraphData| {
        let el_id = graph_data.dom_id_to_render_to.clone();
        let mut graph_data = graph_data.clone();
        async move {
            let mut chart = Chart::new()
                .x_axis(
                    Axis::new()
                        .type_(AxisType::Value)
                        .name_location(NameLocation::Middle)
                        .name_text_style(TextStyle::new().font_size(18.))
                        .name(&graph_data.x_name)
                        .axis_pointer(AxisPointer::new().axis(AxisPointerAxis::X).show(true))
                        .inverse(true)
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
                .data_zoom(
                    DataZoom::new()
                        .type_(DataZoomType::Slider)
                        .start_value(0.)
                        .end_value(5.),
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
                        .trigger(Trigger::Item)
                        .trigger_on(TriggerOn::MousemoveAndClick),
                );
            for series in &graph_data.series {
                chart = chart.series(
                    Scatter::new()
                        .symbol_size(5.)
                        .data(
                            series
                                .x_values
                                .iter()
                                .zip(series.y_values.iter())
                                .map(|(a, b)| {
                                    CompositeValue::Array(vec![
                                        CompositeValue::Number(NumericValue::Float(*a)),
                                        CompositeValue::Number(NumericValue::Float(*b)),
                                    ])
                                })
                                .collect::<Vec<CompositeValue>>(),
                        )
                        .name(&series.name),
                );
            }

            let renderer = WasmRenderer::new(800, 500);
            let chart_instance = renderer.render(el_id.to_string().as_str(), &chart).unwrap();
            let listener = graph_data.click_event_timestamp_receiver.take();
            let series = graph_data.series;
            WasmRenderer::on_event(&chart_instance, "click", move |c| {
                let timestamp = series[c.series_index].original_x_values[c.data_index];
                log!("{:#?}", timestamp);
                log!("{:#?}", c);
                if let Some(l) = listener {
                    l.set(Some(timestamp));
                }
            });
            ()
        }
    })
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

fn create_dom_el_ref_and_graph_call_action(
    data: GraphData,
    create_chart_action: Action<GraphData, ()>,
) -> (NodeRef<Div>, String) {
    let active_traces_graph = NodeRef::<Div>::new();
    let dom_id_to_render_to = data.dom_id_to_render_to.clone();
    active_traces_graph.on_load({
        move |e| {
            create_chart_action.dispatch(data);
        }
    });
    (active_traces_graph, dom_id_to_render_to)
}

fn active_traces_table_el(
    active_trace_graph_click_event_on_timestamp_r: ReadSignal<Option<u64>>,
    service: Rc<ServiceData>,
    root_path: String,
) -> leptos::HtmlElement<Div> {
    let view = move || {
        let timestamp: Option<u64> = match active_trace_graph_click_event_on_timestamp_r.get() {
            None => {
                let mut timestamp = None;
                for i in &service.instances {
                    if let Some(data_point) = i.time_data_points.last() {
                        timestamp = Some(data_point.timestamp);
                        break;
                    }
                }
                timestamp
            }
            Some(timestamp) => Some(timestamp),
        };
        let timestamp = timestamp.unwrap_or(now_nanos_u64());
        let window_secs = 3;
        let window_nanos = 3 * 1000_000_000;
        #[derive(Clone)]
        struct TraceHeaderWithInstance {
            trace_header: TraceHeader,
            instance_id: i64,
        }
        let mut active_traces = vec![];
        let mut finished_traces = vec![];
        for i in &service.instances {
            for d in &i.time_data_points {
                if (timestamp - window_nanos) < d.timestamp
                    && d.timestamp < (timestamp + window_nanos)
                {
                    active_traces.extend_from_slice(
                        &d.active_traces
                            .iter()
                            .map(|trace_header| TraceHeaderWithInstance {
                                trace_header: TraceHeader {
                                    trace_id: trace_header.trace_id,
                                    trace_name: trace_header.trace_name.clone(),
                                    trace_timestamp: trace_header.trace_timestamp,
                                    duration: trace_header.duration,
                                },
                                instance_id: i.id,
                            })
                            .collect::<Vec<TraceHeaderWithInstance>>(),
                    );
                    finished_traces.extend_from_slice(
                        &d.finished_traces
                            .iter()
                            .map(|trace_header| TraceHeaderWithInstance {
                                trace_header: TraceHeader {
                                    trace_id: trace_header.trace_id,
                                    trace_name: trace_header.trace_name.clone(),
                                    trace_timestamp: trace_header.trace_timestamp,
                                    duration: trace_header.duration,
                                },
                                instance_id: i.id,
                            })
                            .collect::<Vec<TraceHeaderWithInstance>>(),
                    );
                }
            }
        }
        let mut active_trace_els = vec![];
        for active in active_traces {
            active_trace_els.push(view! {
                <tr class={"row-container"}>
                    <td class="trace-table__cell">{active.trace_header.trace_name}</td>
                    <td class="trace-table__cell">{active.instance_id}</td>
                    <td class="trace-table__cell">{secs_since(active.trace_header.trace_timestamp)}</td>
                    <td class="trace-table__cell">{active.trace_header.duration.map(|e| (e/1000_000).to_string()).unwrap_or(format!("{} seconds - Still Running", secs_since(active.trace_header.trace_timestamp)))}</td>
                    <td class="trace-table__cell">
                        <a href={format!("{}trace/?service_id={}&trace_id={}&start_timestamp={}", root_path, active.instance_id, active.trace_header.trace_id, active.trace_header.trace_timestamp)}>{"➔"}</a>
                    </td>
                </tr>
            });
        }

        let mut finished_trace_els = vec![];
        for finished in finished_traces {
            finished_trace_els.push(view! {
                <tr class={"row-container"}>
                    <td class="trace-table__cell">{finished.trace_header.trace_name}</td>
                    <td class="trace-table__cell">{finished.instance_id}</td>
                    <td class="trace-table__cell">{secs_since(finished.trace_header.trace_timestamp)}</td>
                    <td class="trace-table__cell">{finished.trace_header.duration.map(|e| (e/1000_000).to_string()).unwrap_or_default()}</td>
                    <td class="trace-table__cell">
                        <a href={format!("{}trace/?service_id={}&trace_id={}&start_timestamp={}", root_path, finished.instance_id, finished.trace_header.trace_id, finished.trace_header.trace_timestamp)}>{"➔"}</a>
                    </td>
                </tr>
            });
        }

        view! {
            <div>
             <p style="text-align: center">{format!("Active Traces {:?} sec ago (+- 3s)   ", secs_since(timestamp))}
             </p>
                    <table class="trace-table">
                        <tr class="row-container">
                            <th style="text-align: center" colspan="5" class="trace-table__cell">
                                <a>"Active"</a>
                            </th>
                        </tr>
                        <tr class="row-container">
                            <th class="trace-table__cell">
                                <a>"Trace Name"</a>
                            </th>
                            <th class="trace-table__cell">
                                <a>"Instance Id"</a>
                            </th>
                            <th class="trace-table__cell">
                                <a>"Secs Ago"</a>
                            </th>
                            <th class="trace-table__cell">
                                <a>{"Duration (ms)"}</a>
                            </th>
                            <th class="trace-table__cell">
                                <a>{"➔"}</a>
                            </th>
                        </tr>
                        {active_trace_els}
                        <tr class="row-container">
                            <th style="text-align: center" colspan="5" class="trace-table__cell">
                                <a>"Finished"</a>
                            </th>
                        </tr>
                        {finished_trace_els}
                    </table>
            </div>
        }
    };
    view! {
        <div>
            {view}
        </div>
    }
}

fn create_active_graph_data(
    instances: &[Instance],
    active_trace_graph_click_event_on_timestamp_w: WriteSignal<Option<u64>>,
) -> GraphData {
    let mut active_trace_series = vec![];
    for instance in instances {
        let mut instance_active_trace_series = GraphSeries::new(format!("active-{}", instance.id));
        for d in &instance.time_data_points {
            instance_active_trace_series.push_data(d.timestamp, d.active_traces.len() as f64);
        }
        active_trace_series.push(instance_active_trace_series);
    }

    for instance in instances {
        let mut received_trace_series = GraphSeries::new(format!("received-{}", instance.id));
        for d in &instance.time_data_points {
            received_trace_series.push_data(
                d.timestamp,
                (d.active_traces.len() + d.finished_traces.len()) as f64,
            );
        }
        active_trace_series.push(received_trace_series);
    }
    let active_traces_graph_data = GraphData {
        dom_id_to_render_to: "active_traces_graph_id".to_string(),
        y_name: "Active and Received Traces".to_string(),
        x_name: "minutes ago".to_string(),
        series: active_trace_series,
        click_event_timestamp_receiver: Some(active_trace_graph_click_event_on_timestamp_w),
    };
    active_traces_graph_data
}

fn create_spe_buffer_graph_data(instances: &[Instance]) -> GraphData {
    let mut series = vec![];
    for instance in instances {
        let mut instance_series = GraphSeries {
            name: instance.id.to_string(),
            original_x_values: vec![],
            x_values: vec![],
            y_values: vec![],
        };
        for d in &instance.time_data_points {
            instance_series.original_x_values.push(d.timestamp);
            let minutes_since = secs_since(d.timestamp) as f64 / 60.;
            instance_series.x_values.push(minutes_since);
            instance_series
                .y_values
                .push(d.tracer_status.spe_buffer_usage as f64);
        }
        series.push(instance_series);
        let mut instance_limit_series = GraphSeries {
            name: format!("{}-spe-buffer-capacity", instance.id.to_string()),
            original_x_values: vec![],
            x_values: vec![],
            y_values: vec![],
        };
        for d in &instance.time_data_points {
            instance_limit_series.original_x_values.push(d.timestamp);
            let minutes_since = secs_since(d.timestamp) as f64 / 60.;
            instance_limit_series.x_values.push(minutes_since);
            instance_limit_series
                .y_values
                .push(d.tracer_status.spe_buffer_capacity as f64);
        }
        series.push(instance_limit_series);
    }
    let graph_data = GraphData {
        dom_id_to_render_to: "spe_buffer_usage_graph_id".to_string(),
        y_name: "SpE Buffer Usage".to_string(),
        x_name: "minutes ago".to_string(),
        series,
        click_event_timestamp_receiver: None,
    };
    graph_data
}

fn create_received_spe_graph_data(instances: &[Instance]) -> GraphData {
    let mut series = vec![];
    for instance in instances {
        let mut instance_series = GraphSeries {
            name: instance.id.to_string(),
            original_x_values: vec![],
            x_values: vec![],
            y_values: vec![],
        };
        for d in &instance.time_data_points {
            instance_series.original_x_values.push(d.timestamp);
            let minutes_since = secs_since(d.timestamp) as f64 / 60.;
            instance_series.x_values.push(minutes_since);
            instance_series.y_values.push(d.received_spe as f64);
        }
        series.push(instance_series);
    }
    let graph_data = GraphData {
        dom_id_to_render_to: "received_spe_graph_id".to_string(),
        y_name: "Received SpE".to_string(),
        x_name: "minutes ago".to_string(),
        series,
        click_event_timestamp_receiver: None,
    };
    graph_data
}

fn create_trace_kbytes_graph_data(instances: &[Instance]) -> GraphData {
    let mut series = vec![];
    for instance in instances {
        let mut instance_series = GraphSeries {
            name: instance.id.to_string(),
            original_x_values: vec![],
            x_values: vec![],
            y_values: vec![],
        };
        for d in &instance.time_data_points {
            instance_series.original_x_values.push(d.timestamp);
            let minutes_since = secs_since(d.timestamp) as f64 / 60.;
            instance_series.x_values.push(minutes_since);
            instance_series
                .y_values
                .push((d.received_trace_bytes as f64) / 1000.0);
        }
        series.push(instance_series);
    }
    let graph_data = GraphData {
        dom_id_to_render_to: "received_trace_kb_graph_id".to_string(),
        y_name: "Received Trace kb".to_string(),
        x_name: "minutes ago".to_string(),
        series,
        click_event_timestamp_receiver: None,
    };
    graph_data
}

fn create_trace_spe_usage_graph_data(instances: &[Instance]) -> GraphData {
    let mut series = vec![];
    for instance in instances {
        let mut max_usage_per_trace_name: HashMap<String, u64> = HashMap::new();
        for d in &instance.time_data_points {
            for (trace_name, status) in &d.tracer_status.per_minute_trace_stats {
                let curr = max_usage_per_trace_name
                    .entry(trace_name.clone())
                    .or_default();
                if *curr < status.spe_usage_per_minute {
                    *curr = status.spe_usage_per_minute;
                }
            }
        }
        let mut max_usage_per_trace_name_sorted: Vec<(String, u64)> =
            max_usage_per_trace_name.into_iter().collect();
        max_usage_per_trace_name_sorted.sort_by_key(|(name, usage)| *usage);
        max_usage_per_trace_name_sorted.reverse();
        let top_3_trace_names: Vec<String> = max_usage_per_trace_name_sorted
            .into_iter()
            .take(3)
            .map(|e| e.0)
            .collect();

        for trace_name in top_3_trace_names {
            let mut trace_series = GraphSeries {
                name: format!("{trace_name}-{}", instance.id.to_string()),
                original_x_values: vec![],
                x_values: vec![],
                y_values: vec![],
            };
            for d in &instance.time_data_points {
                if let Some(status) = d.tracer_status.per_minute_trace_stats.get(&trace_name) {
                    trace_series.original_x_values.push(d.timestamp);
                    let minutes_since = secs_since(d.timestamp) as f64 / 60.;
                    trace_series.x_values.push(minutes_since);
                    trace_series
                        .y_values
                        .push(status.spe_usage_per_minute as f64);
                }
            }
            series.push(trace_series);
        }
    }
    let graph_data = GraphData {
        dom_id_to_render_to: "spe_usage_per_minute_graph_id".to_string(),
        y_name: "SpE Usage Per Min Per Trace".to_string(),
        x_name: "minutes ago".to_string(),
        series,
        click_event_timestamp_receiver: None,
    };
    graph_data
}

#[component]
pub fn ServicesStatistics(root_path: String) -> impl IntoView {
    let (services_r, services_w) = leptos::create_signal(Option::<Vec<ServiceData>>::None);
    let (instance_idx_r, instance_idx_w) = leptos::create_signal(Option::<usize>::None);
    let (
        active_trace_graph_click_event_on_timestamp_r,
        active_trace_graph_click_event_on_timestamp_w,
    ) = leptos::create_signal(Option::<u64>::None);
    let _api_request_sender =
        leptos::create_local_resource(move || (), move |_| get_summary(services_w));
    let change_rust_log_action =
        create_action(move |new_filters: &NewFiltersRequest| update_filter(new_filters.clone()));
    let create_chart_action = create_chart_action();

    let view = move || match services_r.get() {
        None => {
            view! {
                <div style="padding: 20px; color: white">
                   <p>"Loading, maybe failed, check logs"</p>
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
            let mut services_els = vec![];
            for service in services {
                let service = Rc::new(service);
                let service_name = service.name.clone();
                let env = service.env;
                let instance_specific_data_els = {
                    let mut instance_specific_data_els = vec![];
                    for instance in &service.instances {
                        let els = instance_specific_data_ui(&instance, change_rust_log_action);
                        instance_specific_data_els.push(els);
                    }
                    instance_specific_data_els
                };
                let active_traces_graph_data = create_active_graph_data(
                    &service.instances,
                    active_trace_graph_click_event_on_timestamp_w,
                );
                let (active_traces_graph, active_traces_graph_id): (NodeRef<Div>, String) =
                    create_dom_el_ref_and_graph_call_action(
                        active_traces_graph_data,
                        create_chart_action,
                    );
                // SPE BUFFER
                let spe_buffer_graph_data = create_spe_buffer_graph_data(&service.instances);
                let (spe_buffer_usage, spe_buffer_usage_graph_id): (NodeRef<Div>, String) =
                    create_dom_el_ref_and_graph_call_action(
                        spe_buffer_graph_data,
                        create_chart_action,
                    );

                //
                let spe_usage_graph_data = create_trace_spe_usage_graph_data(&service.instances);
                let (trace_spe_usage, trace_spe_usage_graph_id): (NodeRef<Div>, String) =
                    create_dom_el_ref_and_graph_call_action(
                        spe_usage_graph_data,
                        create_chart_action,
                    );
                //
                let received_spe_graph_data = create_received_spe_graph_data(&service.instances);
                let (received_spe_graph, received_spe_graph_id): (NodeRef<Div>, String) =
                    create_dom_el_ref_and_graph_call_action(
                        received_spe_graph_data,
                        create_chart_action,
                    );
                //
                let trace_kbytes_graph_data = create_trace_kbytes_graph_data(&service.instances);
                let (trace_kbytes_graph, trace_kbytes_graph_id): (NodeRef<Div>, String) =
                    create_dom_el_ref_and_graph_call_action(
                        trace_kbytes_graph_data,
                        create_chart_action,
                    );

                //
                //
                let active_services_el = active_traces_table_el(
                    active_trace_graph_click_event_on_timestamp_r,
                    Rc::clone(&service),
                    root_path.clone(),
                );
                services_els.push(view! {
                    <div>
                        <h2 style="text-align: center">{format!("Service: {service_name} at {env}")}</h2>
                            {instance_specific_data_els}
                            <div style="display: flex; flex-wrap: wrap; justify-content: center; margin: 10px 0 10px 0">
                                <div _ref=spe_buffer_usage id=spe_buffer_usage_graph_id.clone()></div>
                                <div _ref=trace_spe_usage id=trace_spe_usage_graph_id.clone()></div>
                                <div _ref=received_spe_graph id=received_spe_graph_id.clone()></div>
                                <div _ref=trace_kbytes_graph id=trace_kbytes_graph_id.clone()></div>
                                <div _ref=active_traces_graph id=active_traces_graph_id.clone()></div>
                                // <div _ref=export_buffer_graph id=export_buffer_graph_id.clone()></div>
                                // <div _ref=logs_dropped_per_min_graph id=logs_dropped_per_min_graph_id.clone()></div>
                                // <div _ref=dropped_traces_per_min_graph id=dropped_traces_per_min_graph_id.clone()></div>
                                // <div _ref=events_kb_per_min_graph id=events_kb_per_min_graph_id.clone()></div>
                            </div>
                            {active_services_el}
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
                    </div>
                });
            }
            view! {
                <div style="padding: 20px; color: white">
                    {services_els}
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

async fn get_summary(w: WriteSignal<Option<Vec<api_structs::ui::service_health::ServiceData>>>) {
    log!("Sending req");
    let traces: Vec<api_structs::ui::service_health::ServiceData> = gloo_net::http::Request::get(
        &format!("{}/api/instances", API_SERVER_URL_NO_TRAILING_SLASH),
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    log!("Got summary back");
    w.set(Some(traces));
}
