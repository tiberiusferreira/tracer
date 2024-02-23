use crate::services::graph_creation::{
    create_dom_el_ref_and_graph_call_action, GraphData, GraphSeries,
};
use api_structs::ui::service::ServiceDataOverTime;
use leptos::html::Div;
use leptos::{Action, NodeRef, WriteSignal};

fn create_budget_usage_kbytes_graph_data(
    instances: &[ServiceDataOverTime],
    click_timestamp_receiver: WriteSignal<Option<u64>>,
) -> GraphData {
    let mut series = GraphSeries::new("budget_used_kbs".to_string());
    for d in instances {
        let data = d.traces_budget_usage.values().sum::<u32>() + d.orphan_events_budget_usage;
        series.push_data(d.timestamp, (data / 1000) as f64);
    }
    let graph_data = GraphData {
        dom_id_to_render_to: "budget_used_kbs_graph_id".to_string(),
        y_name: "Budget Used kb".to_string(),
        x_name: "minutes ago".to_string(),
        series: vec![series],
        click_event_timestamp_receiver: Some(click_timestamp_receiver),
    };
    graph_data
}

pub fn create_budget_usage_kbytes_graph(
    instances: &[ServiceDataOverTime],
    click_timestamp_receiver: WriteSignal<Option<u64>>,
    create_chart_action: Action<GraphData, ()>,
) -> (NodeRef<Div>, String) {
    let graph_data = create_budget_usage_kbytes_graph_data(&instances, click_timestamp_receiver);
    create_dom_el_ref_and_graph_call_action(graph_data, create_chart_action)
}
