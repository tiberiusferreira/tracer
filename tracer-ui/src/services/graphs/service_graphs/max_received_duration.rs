use crate::services::graph_creation::{
    create_dom_el_ref_and_graph_call_action, GraphData, GraphSeries,
};
use api_structs::ui::service::{Instance, ServiceDataOverTime};
use leptos::html::Div;
use leptos::{Action, NodeRef, WriteSignal};

pub fn create_graph(
    instances: &[ServiceDataOverTime],
    click_timestamp_receiver: WriteSignal<Option<u64>>,
    create_chart_action: Action<GraphData, ()>,
) -> (NodeRef<Div>, String) {
    let graph_data = create_graph_data(&instances, click_timestamp_receiver);
    create_dom_el_ref_and_graph_call_action(graph_data, create_chart_action)
}

fn create_graph_data(
    instances: &[ServiceDataOverTime],
    click_timestamp_receiver: WriteSignal<Option<u64>>,
) -> GraphData {
    let mut series = vec![];
    for d in instances {
        let mut single_series = GraphSeries::new("duration".to_string());
        let max = d.finished_traces.iter().filter_map(|d| d.duration).max();
        if let Some(max_duration) = max {
            single_series.push_data(d.timestamp, max_duration as f64 / 1000_000_000.);
        }
        series.push(single_series);
    }

    GraphData {
        dom_id_to_render_to: "max_received_trace_duration_graph_id".to_string(),
        y_name: "Max Received Trace Duration (s)".to_string(),
        x_name: "minutes ago".to_string(),
        series,
        click_event_timestamp_receiver: Some(click_timestamp_receiver),
    }
}
