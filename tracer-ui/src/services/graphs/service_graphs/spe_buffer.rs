use crate::datetime::secs_since;
use crate::services::graph_creation::{
    create_dom_el_ref_and_graph_call_action, GraphData, GraphSeries,
};
use api_structs::ui::service::Instance;
use leptos::html::Div;
use leptos::{Action, NodeRef, WriteSignal};

fn create_graph_data(
    instances: &[Instance],
    click_timestamp_receiver: WriteSignal<Option<u64>>,
) -> GraphData {
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
            name: format!("spe-buffer-capacity-{}", instance.id.to_string()),
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
        y_name: "SpE Export Buffer Usage".to_string(),
        x_name: "minutes ago".to_string(),
        series,
        click_event_timestamp_receiver: Some(click_timestamp_receiver),
    };
    graph_data
}

pub fn create_graph(
    instances: &[Instance],
    click_timestamp_receiver: WriteSignal<Option<u64>>,
    create_chart_action: Action<GraphData, ()>,
) -> (NodeRef<Div>, String) {
    let graph_data = create_graph_data(&instances, click_timestamp_receiver);
    create_dom_el_ref_and_graph_call_action(graph_data, create_chart_action)
}
