pub mod service_graphs;
pub mod trace_details_graphs;

// fn create_received_orphan_event_bytes_graph_data(
//     instances: &[Instance],
//     click_timestamp_receiver: WriteSignal<Option<u64>>,
// ) -> GraphData {
//     let mut series = vec![];
//     for instance in instances {
//         let mut instance_series = GraphSeries {
//             name: instance.id.to_string(),
//             original_x_values: vec![],
//             x_values: vec![],
//             y_values: vec![],
//         };
//         for d in &instance.time_data_points {
//             instance_series.push_data(d.timestamp, (d.received_orphan_event_bytes as f64) / 1000.0);
//         }
//         series.push(instance_series);
//     }
//     let graph_data = GraphData {
//         dom_id_to_render_to: "received_orphan_event_bytes_graph_id".to_string(),
//         y_name: "Received Orphan Event kb".to_string(),
//         x_name: "minutes ago".to_string(),
//         series,
//         click_event_timestamp_receiver: Some(click_timestamp_receiver),
//     };
//     graph_data
// }
//
// fn create_orphan_events_per_minute_usage_graph_data(
//     instances: &[Instance],
//     click_timestamp_receiver: WriteSignal<Option<u64>>,
// ) -> GraphData {
//     let mut series = vec![];
//     for instance in instances {
//         let mut instance_series = GraphSeries {
//             name: instance.id.to_string(),
//             original_x_values: vec![],
//             x_values: vec![],
//             y_values: vec![],
//         };
//         let mut instance_limit_series = GraphSeries {
//             name: format!("limit-{}", instance.id),
//             original_x_values: vec![],
//             x_values: vec![],
//             y_values: vec![],
//         };
//         for d in &instance.time_data_points {
//             instance_series.push_data(
//                 d.timestamp,
//                 d.sampling.orphan_events_sampling_rate_0_to_1 as f64,
//             );
//         }
//         series.push(instance_series);
//         series.push(instance_limit_series);
//     }
//     let graph_data = GraphData {
//         dom_id_to_render_to: "orphan_events_per_minute_graph_id".to_string(),
//         y_name: "Orphan Events Per Min".to_string(),
//         x_name: "minutes ago".to_string(),
//         series,
//         click_event_timestamp_receiver: Some(click_timestamp_receiver),
//     };
//     graph_data
// }

// fn create_orphan_events_dropped_by_sampling_per_minute_graph_data(
//     instances: &[Instance],
//     click_timestamp_receiver: WriteSignal<Option<u64>>,
// ) -> GraphData {
//     let mut series = vec![];
//     for instance in instances {
//         let mut instance_series = GraphSeries {
//             name: instance.id.to_string(),
//             original_x_values: vec![],
//             x_values: vec![],
//             y_values: vec![],
//         };
//         for d in &instance.time_data_points {
//             instance_series.push_data(
//                 d.timestamp,
//                 d.tracer_status.orphan_events_dropped_by_sampling_per_minute as f64,
//             );
//         }
//         series.push(instance_series);
//     }
//     let graph_data = GraphData {
//         dom_id_to_render_to: "orphan_events_dropped_by_sampling_per_minute_graph_id".to_string(),
//         y_name: "Orphan Events Dropped By Sampling Per Min".to_string(),
//         x_name: "minutes ago".to_string(),
//         series,
//         click_event_timestamp_receiver: Some(click_timestamp_receiver),
//     };
//     graph_data
// }

// fn create_spe_dropped_due_to_full_export_buffer_graph_data(
//     instances: &[Instance],
//     click_timestamp_receiver: WriteSignal<Option<u64>>,
// ) -> GraphData {
//     let mut series = vec![];
//     for instance in instances {
//         let mut instance_series = GraphSeries {
//             name: instance.id.to_string(),
//             original_x_values: vec![],
//             x_values: vec![],
//             y_values: vec![],
//         };
//         for d in &instance.time_data_points {
//             instance_series.push_data(
//                 d.timestamp,
//                 d.tracer_status
//                     .spe_dropped_due_to_full_export_buffer_per_min as f64,
//             );
//         }
//         series.push(instance_series);
//     }
//     let graph_data = GraphData {
//         dom_id_to_render_to: "spe_dropped_due_to_full_export_buffer_per_min_graph_id".to_string(),
//         y_name: "SpE Dropped Due To Full Export Buffer".to_string(),
//         x_name: "minutes ago".to_string(),
//         series,
//         click_event_timestamp_receiver: Some(click_timestamp_receiver),
//     };
//     graph_data
// }

// fn create_trace_spe_usage_graph_data(
//     instances: &[Instance],
//     click_timestamp_receiver: WriteSignal<Option<u64>>,
// ) -> GraphData {
//     let mut series = vec![];
//     for instance in instances {
//         let mut usage_limit_series = GraphSeries {
//             name: format!("limit-{}", instance.id.to_string()),
//             original_x_values: vec![],
//             x_values: vec![],
//             y_values: vec![],
//         };
//
//         let mut max_usage_per_trace_name: HashMap<String, u64> = HashMap::new();
//         for d in &instance.time_data_points {
//             usage_limit_series.push_data(
//                 d.timestamp,
//                 d.tracer_status
//                     .sampler_limits
//                     .trace_spe_per_minute_per_trace_limit as f64,
//             );
//             for (trace_name, status) in &d.tracer_status.per_minute_trace_stats {
//                 let curr = max_usage_per_trace_name
//                     .entry(trace_name.clone())
//                     .or_default();
//                 if *curr < status.spe_usage_per_minute {
//                     *curr = status.spe_usage_per_minute;
//                 }
//             }
//         }
//         series.push(usage_limit_series);
//         let mut max_usage_per_trace_name_sorted: Vec<(String, u64)> =
//             max_usage_per_trace_name.into_iter().collect();
//         max_usage_per_trace_name_sorted.sort_by_key(|(_name, usage)| *usage);
//         max_usage_per_trace_name_sorted.reverse();
//         let top_3_trace_names: Vec<String> = max_usage_per_trace_name_sorted
//             .into_iter()
//             .take(3)
//             .map(|e| e.0)
//             .collect();
//
//         for trace_name in top_3_trace_names {
//             let mut trace_series = GraphSeries {
//                 name: format!("{trace_name}-{}", instance.id.to_string()),
//                 original_x_values: vec![],
//                 x_values: vec![],
//                 y_values: vec![],
//             };
//             for d in &instance.time_data_points {
//                 if let Some(status) = d.tracer_status.per_minute_trace_stats.get(&trace_name) {
//                     trace_series.original_x_values.push(d.timestamp);
//                     let minutes_since = secs_since(d.timestamp) as f64 / 60.;
//                     trace_series.x_values.push(minutes_since);
//                     trace_series
//                         .y_values
//                         .push(status.spe_usage_per_minute as f64);
//                 }
//             }
//             series.push(trace_series);
//         }
//     }
//     let graph_data = GraphData {
//         dom_id_to_render_to: "spe_usage_per_minute_graph_id".to_string(),
//         y_name: "SpE Usage Per Min Per Trace".to_string(),
//         x_name: "minutes ago".to_string(),
//         series,
//         click_event_timestamp_receiver: Some(click_timestamp_receiver),
//     };
//     graph_data
// }

// pub fn create_trace_spe_usage_traces_graph(
//     instances: &[Instance],
//     click_timestamp_receiver: WriteSignal<Option<u64>>,
//     create_chart_action: Action<GraphData, ()>,
// ) -> (NodeRef<Div>, String) {
//     let graph_data = create_trace_spe_usage_graph_data(&instances, click_timestamp_receiver);
//     create_dom_el_ref_and_graph_call_action(graph_data, create_chart_action)
// }

// pub fn create_received_trace_kbytes_graph(
//     instances: &[Instance],
//     click_timestamp_receiver: WriteSignal<Option<u64>>,
//     create_chart_action: Action<GraphData, ()>,
// ) -> (NodeRef<Div>, String) {
//     let graph_data = create_received_trace_kbytes_graph_data(&instances, click_timestamp_receiver);
//     create_dom_el_ref_and_graph_call_action(graph_data, create_chart_action)
// }
//
// pub fn create_received_orphan_event_bytes_graph(
//     instances: &[Instance],
//     click_timestamp_receiver: WriteSignal<Option<u64>>,
//     create_chart_action: Action<GraphData, ()>,
// ) -> (NodeRef<Div>, String) {
//     let graph_data =
//         create_received_orphan_event_bytes_graph_data(&instances, click_timestamp_receiver);
//     create_dom_el_ref_and_graph_call_action(graph_data, create_chart_action)
// }

// pub fn create_orphan_events_per_minute_usage_graph(
//     instances: &[Instance],
//     click_timestamp_receiver: WriteSignal<Option<u64>>,
//     create_chart_action: Action<GraphData, ()>,
// ) -> (NodeRef<Div>, String) {
//     let graph_data =
//         create_orphan_events_per_minute_usage_graph_data(&instances, click_timestamp_receiver);
//     create_dom_el_ref_and_graph_call_action(graph_data, create_chart_action)
// }

// pub fn create_spe_dropped_due_to_full_export_buffer_graph(
//     instances: &[Instance],
//     click_timestamp_receiver: WriteSignal<Option<u64>>,
//     create_chart_action: Action<GraphData, ()>,
// ) -> (NodeRef<Div>, String) {
//     let graph_data = create_spe_dropped_due_to_full_export_buffer_graph_data(
//         &instances,
//         click_timestamp_receiver,
//     );
//     create_dom_el_ref_and_graph_call_action(graph_data, create_chart_action)
// }
//
