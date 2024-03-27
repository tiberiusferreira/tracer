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
    let mut active_trace_series = vec![];
    for d in instances {
        let mut received_trace_series = GraphSeries::new("received".to_string());
        received_trace_series.push_data(
            d.timestamp,
            (d.traces_state.len() + d.finished_traces().count()) as f64,
        );
        active_trace_series.push(received_trace_series);
    }

    for d in instances {
        let mut received_trace_series = GraphSeries::new("errors".to_string());
        received_trace_series.push_data(
            d.timestamp,
            d.traces_state.iter().filter(|t| t.new_errors).count() as f64,
        );
        active_trace_series.push(received_trace_series);
    }

    for d in instances {
        let mut instance_active_trace_series = GraphSeries::new("active".to_string());
        instance_active_trace_series.push_data(d.timestamp, d.traces_state.len() as f64);
        active_trace_series.push(instance_active_trace_series);
    }

    for d in instances {
        let mut received_trace_series = GraphSeries::new("warnings".to_string());
        received_trace_series.push_data(
            d.timestamp,
            d.traces_state.iter().filter(|t| t.new_warnings).count() as f64,
        );
        active_trace_series.push(received_trace_series);
    }

    let active_traces_graph_data = GraphData {
        dom_id_to_render_to: "active_traces_graph_id".to_string(),
        y_name: "Active Finished Warning Error Update Count".to_string(),
        x_name: "minutes ago".to_string(),
        series: active_trace_series,
        click_event_timestamp_receiver: Some(click_timestamp_receiver),
    };
    active_traces_graph_data
}

pub fn create_graph(
    instances: &[ServiceDataOverTime],
    click_timestamp_receiver: WriteSignal<Option<u64>>,
    create_chart_action: Action<GraphData, ()>,
) -> (NodeRef<Div>, String) {
    let active_traces_graph_data = create_graph_data(&instances, click_timestamp_receiver);
    create_dom_el_ref_and_graph_call_action(active_traces_graph_data, create_chart_action)
}
