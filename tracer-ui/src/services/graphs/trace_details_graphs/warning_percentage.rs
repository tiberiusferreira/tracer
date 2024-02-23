// use crate::services::graph_creation::{
//     create_dom_el_ref_and_graph_call_action, GraphData, GraphSeries,
// };
// use api_structs::time_conversion::nanos_to_millis;
// use api_structs::ui::service::Instance;
// use leptos::html::Div;
// use leptos::{Action, NodeRef, WriteSignal};
// use tracing::debug;
//
// fn create_graph_data(
//     instances: &[Instance],
//     trace_name: String,
//     window_seconds: u64,
//     min_samples: u64,
//     click_timestamp_receiver: WriteSignal<Option<u64>>,
// ) -> GraphData {
//     #[derive(Debug, Clone)]
//     struct WarningDataPoint {
//         timestamp: u64,
//         total: usize,
//         warnings: usize,
//     }
//     let mut warnings_over_time = vec![];
//     let mut warnings_series = GraphSeries::new(format!("warnings-percentage-{trace_name}"));
//     for instance in instances {
//         for d in &instance.time_data_points {
//             let with_warning_count = d
//                 .active_and_finished_iter()
//                 .filter(|d| d.trace_name == trace_name && d.new_warnings)
//                 .count();
//             if with_warning_count == 0 {
//                 continue;
//             }
//             let total_count = d
//                 .active_and_finished_iter()
//                 .filter(|d| d.trace_name == trace_name)
//                 .count();
//             warnings_over_time.push(WarningDataPoint {
//                 timestamp: d.timestamp,
//                 total: total_count,
//                 warnings: with_warning_count,
//             });
//         }
//     }
//     debug!(warnings_over_time_len = warnings_over_time.len());
//     warnings_over_time.sort_by_key(|w| w.timestamp);
//     for (mut idx, window_start) in warnings_over_time.iter().enumerate() {
//         debug!(?window_start, "checking window for");
//         let mut window = vec![window_start];
//         while let Some(next_window_data_point) = warnings_over_time.get(idx + 1) {
//             if ((next_window_data_point.timestamp - window_start.timestamp) / 1_000_000_000)
//                 < window_seconds
//             {
//                 window.push(next_window_data_point);
//                 debug!(window_len = window.len());
//             } else {
//                 break;
//             }
//             idx = idx + 1;
//         }
//         debug!(?window);
//         let last_window_data_point = window.last().expect("window to have at least 1 element");
//         if window.len() >= (min_samples as usize) {
//             debug!("window over min_sample size, using it");
//             let total: usize = window.iter().map(|e| e.total).sum();
//             let warnings: usize = window.iter().map(|e| e.warnings).sum();
//             warnings_series.push_data(
//                 last_window_data_point.timestamp,
//                 100. * warnings as f64 / total as f64,
//             );
//         } else {
//             debug!("window below min_sample size, discarding");
//         }
//         if nanos_to_millis(last_window_data_point.timestamp - window_start.timestamp)
//             < window_seconds
//         {
//             debug!("got the min amount of samples, but they were shorter than the window size, skipping rest");
//             break;
//         }
//     }
//
//     GraphData {
//         dom_id_to_render_to: "trace_warning_percentage_graph_id".to_string(),
//         y_name: "Trace Warning %".to_string(),
//         x_name: "minutes ago".to_string(),
//         series: vec![warnings_series],
//         click_event_timestamp_receiver: Some(click_timestamp_receiver),
//     }
// }
//
// pub fn create_graph(
//     instances: &[Instance],
//     trace_name: String,
//     window_seconds: u64,
//     min_samples: u64,
//     click_timestamp_receiver: WriteSignal<Option<u64>>,
//     create_chart_action: Action<GraphData, ()>,
// ) -> (NodeRef<Div>, String) {
//     let trace_warning_graph_data = create_graph_data(
//         &instances,
//         trace_name,
//         window_seconds,
//         min_samples,
//         click_timestamp_receiver,
//     );
//     create_dom_el_ref_and_graph_call_action(trace_warning_graph_data, create_chart_action)
// }
