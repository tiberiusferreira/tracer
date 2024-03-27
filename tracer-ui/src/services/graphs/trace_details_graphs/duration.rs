use crate::services::graph_creation::{
    create_dom_el_ref_and_graph_call_action, GraphData, GraphSeries,
};
use api_structs::time_conversion::nanos_to_millis;
use api_structs::ui::service::{Instance, ServiceDataOverTime};
use leptos::html::Div;
use leptos::{Action, NodeRef, WriteSignal};

fn create_graph_data(
    instances: &[ServiceDataOverTime],
    trace_name: String,
    click_timestamp_receiver: WriteSignal<Option<u64>>,
) -> GraphData {
    let mut duration_series = GraphSeries::new("duration".to_string());
    for d in instances {
        for t in &d.traces_state {
            if t.trace_name == trace_name {
                if let Some(duration) = t.duration {
                    duration_series.push_data(d.timestamp, nanos_to_millis(duration) as f64);
                }
            }
        }
    }

    GraphData {
        dom_id_to_render_to: "trace_duration_graph_id".to_string(),
        y_name: "Duration (ms)".to_string(),
        x_name: "minutes ago".to_string(),
        series: vec![duration_series],
        click_event_timestamp_receiver: Some(click_timestamp_receiver),
    }
}

pub fn create_graph(
    instances: &[ServiceDataOverTime],
    trace_name: String,
    click_timestamp_receiver: WriteSignal<Option<u64>>,
    create_chart_action: Action<GraphData, ()>,
) -> (NodeRef<Div>, String) {
    let graph_data = create_graph_data(&instances, trace_name, click_timestamp_receiver);
    create_dom_el_ref_and_graph_call_action(graph_data, create_chart_action)
}
