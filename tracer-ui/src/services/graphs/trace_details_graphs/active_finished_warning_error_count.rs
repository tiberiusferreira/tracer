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
    let mut active_series = GraphSeries::new("active".to_string());
    let mut finished_series = GraphSeries::new("finished".to_string());
    let mut active_and_finished_series = GraphSeries::new("active_and_finished".to_string());
    let mut warnings_series = GraphSeries::new("warnings".to_string());
    let mut errors_series = GraphSeries::new("errors".to_string());
    for instance in instances {
        for d in &instance.time_data_points {
            let active_count = d
                .active_traces
                .iter()
                .filter(|d| d.trace_name == trace_name)
                .count();
            active_series.push_data(d.timestamp, active_count as f64);
            let finished_count = d
                .finished_traces
                .iter()
                .filter(|d| d.trace_name == trace_name)
                .count();
            finished_series.push_data(d.timestamp, finished_count as f64);
            active_and_finished_series
                .push_data(d.timestamp, (active_count + finished_count) as f64);
            let with_error_count = d
                .active_and_finished_iter()
                .filter(|d| d.trace_name == trace_name && d.new_errors)
                .count();
            errors_series.push_data(d.timestamp, with_error_count as f64);
            let with_warning_count = d
                .active_and_finished_iter()
                .filter(|d| d.trace_name == trace_name && d.new_warnings)
                .count();
            warnings_series.push_data(d.timestamp, with_warning_count as f64);
        }
    }

    GraphData {
        dom_id_to_render_to: "trace_active_finished_warning_error_count_graph_id".to_string(),
        y_name: "Active Finished Warning Error Update Count".to_string(),
        x_name: "minutes ago".to_string(),
        series: vec![
            active_series,
            finished_series,
            active_and_finished_series,
            warnings_series,
            errors_series,
        ],
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
