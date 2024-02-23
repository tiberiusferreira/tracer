use crate::datetime::secs_since;
use crate::services::graph_creation::{
    create_dom_el_ref_and_graph_call_action, GraphData, GraphSeries,
};
use api_structs::ui::service::{Instance, ServiceDataOverTime};
use leptos::html::Div;
use leptos::{Action, NodeRef, WriteSignal};

fn create_graph_data(
    instances: &[ServiceDataOverTime],
    click_timestamp_receiver: WriteSignal<Option<u64>>,
) -> GraphData {
    let mut series = vec![];
    for d in instances {
        let mut instance_series = GraphSeries::new("export_buffer_usage".to_string());
        instance_series.push_data(
            d.timestamp,
            d.export_buffer_stats.export_buffer_usage as f64,
        );
        series.push(instance_series);
    }
    let graph_data = GraphData {
        dom_id_to_render_to: "export_buffer_usage_graph_id".to_string(),
        y_name: "Export Buffer Usage".to_string(),
        x_name: "minutes ago".to_string(),
        series,
        click_event_timestamp_receiver: Some(click_timestamp_receiver),
    };
    graph_data
}

pub fn create_graph(
    instances: &[ServiceDataOverTime],
    click_timestamp_receiver: WriteSignal<Option<u64>>,
    create_chart_action: Action<GraphData, ()>,
) -> (NodeRef<Div>, String) {
    let graph_data = create_graph_data(&instances, click_timestamp_receiver);
    create_dom_el_ref_and_graph_call_action(graph_data, create_chart_action)
}
