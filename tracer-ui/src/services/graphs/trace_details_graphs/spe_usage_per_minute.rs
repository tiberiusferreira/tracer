use crate::services::graph_creation::{
    create_dom_el_ref_and_graph_call_action, GraphData, GraphSeries,
};
use api_structs::ui::service::Instance;
use leptos::html::Div;
use leptos::{Action, NodeRef, WriteSignal};

fn create_graph_data(
    instances: &[Instance],
    trace_name: String,
    click_timestamp_receiver: WriteSignal<Option<u64>>,
) -> GraphData {
    let mut series = vec![];
    for instance in instances {
        let mut instance_series = GraphSeries::new(format!("spe_usage_per_min-{}", instance.id));
        for d in &instance.time_data_points {
            if let Some(single_trace_status) =
                d.tracer_status.per_minute_trace_stats.get(&trace_name)
            {
                instance_series
                    .push_data(d.timestamp, single_trace_status.spe_usage_per_minute as f64);
            }
        }
        series.push(instance_series);
    }

    GraphData {
        dom_id_to_render_to: "spe_usage_per_min_graph_id".to_string(),
        y_name: "SpE Usage Per Minute".to_string(),
        x_name: "minutes ago".to_string(),
        series,
        click_event_timestamp_receiver: Some(click_timestamp_receiver),
    }
}

pub fn create_graph(
    instances: &[Instance],
    trace_name: String,
    click_timestamp_receiver: WriteSignal<Option<u64>>,
    create_chart_action: Action<GraphData, ()>,
) -> (NodeRef<Div>, String) {
    let graph_data = create_graph_data(&instances, trace_name, click_timestamp_receiver);
    create_dom_el_ref_and_graph_call_action(graph_data, create_chart_action)
}
