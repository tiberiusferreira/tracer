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

    for instance in instances {
        let mut received_trace_series = GraphSeries::new(format!("warnings-{}", instance.id));
        for d in &instance.time_data_points {
            received_trace_series.push_data(
                d.timestamp,
                d.active_traces
                    .iter()
                    .chain(d.finished_traces.iter())
                    .filter(|t| t.new_warnings)
                    .count() as f64,
            );
        }
        active_trace_series.push(received_trace_series);
    }

    for instance in instances {
        let mut received_trace_series = GraphSeries::new(format!("errors-{}", instance.id));
        for d in &instance.time_data_points {
            received_trace_series.push_data(
                d.timestamp,
                d.active_traces
                    .iter()
                    .chain(d.finished_traces.iter())
                    .filter(|t| t.new_errors)
                    .count() as f64,
            );
        }
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
    instances: &[Instance],
    click_timestamp_receiver: WriteSignal<Option<u64>>,
    create_chart_action: Action<GraphData, ()>,
) -> (NodeRef<Div>, String) {
    let active_traces_graph_data = create_graph_data(&instances, click_timestamp_receiver);
    create_dom_el_ref_and_graph_call_action(active_traces_graph_data, create_chart_action)
}
