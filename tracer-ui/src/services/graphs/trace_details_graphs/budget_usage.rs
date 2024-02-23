use crate::services::graph_creation::{
    create_dom_el_ref_and_graph_call_action, GraphData, GraphSeries,
};
use api_structs::ui::service::{Instance, ServiceDataOverTime};
use leptos::html::Div;
use leptos::{Action, NodeRef, WriteSignal};

fn create_graph_data(
    instances: &[ServiceDataOverTime],
    trace_name: String,
    click_timestamp_receiver: WriteSignal<Option<u64>>,
) -> GraphData {
    let mut series = GraphSeries::new("budget-usage-kb".to_string());
    for d in instances {
        if let Some(trace_usage) = d.traces_budget_usage.get(&trace_name) {
            series.push_data(d.timestamp, (trace_usage / 1000) as f64);
        }
    }
    GraphData {
        dom_id_to_render_to: "budget_usage_graph_id".to_string(),
        y_name: "Budget Usage kb".to_string(),
        x_name: "minutes ago".to_string(),
        series: vec![series],
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
