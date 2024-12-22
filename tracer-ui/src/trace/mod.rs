use crate::chart;
use crate::chart::{CategoryGraphSeries, StackedBarGraphData};
use api_structs::ui::chart::SeriesData;
use leptos::prelude::*;
use leptos::{component, view};
use std::string::ToString;

#[component]
pub fn TraceBrowserPage() -> impl IntoView {
    let mut time_bucket_chart = api_structs::ui::chart::TimeBucketChart {
        x_axis_label: "time[1h]".to_string(),
        y_axis_label: "Requests".to_string(),
        x_time_buckets: vec![],
        y_data: vec![
            SeriesData {
                series_name: "Ok".to_string(),
                data: vec![],
            },
            SeriesData {
                series_name: "Error".to_string(),
                data: vec![],
            },
        ],
    };

    let max_value = 12;
    for i in 0..=max_value {
        let datetime = chrono::prelude::Utc::now() - chrono::Duration::hours(max_value - i);
        time_bucket_chart.x_time_buckets.push(datetime);
        time_bucket_chart.y_data[0].data.push(i as f64);
        time_bucket_chart.y_data[1].data.push(i as f64);
    }
    let w = time_bucket_chart
        .y_data
        .iter()
        .map(|e: &SeriesData| CategoryGraphSeries {
            name: e.series_name.clone(),
            category_values: time_bucket_chart
                .x_time_buckets
                .iter()
                .map(|e| e.format("%d %H:%M").to_string())
                .collect(),
            y_values: e.data.clone(),
        })
        .collect();
    let data = StackedBarGraphData {
        dom_id_to_render_to: "test".to_string(),
        x_axis_label: time_bucket_chart.x_axis_label.clone(),
        y_axis_label: time_bucket_chart.y_axis_label.clone(),
        series: w,
        click_event_timestamp_receiver: None,
    };
    let action = chart::create_create_chart_action();
    let (trace_warning_graph, trace_warning_graph_id) =
        chart::create_dom_el_ref_and_graph_call_action(data, action);
    view! {
        <div
        style="display: flex; flex-wrap: wrap; justify-content: center; margin: 5px 0 5px 0;\
                border: 1px solid; padding: 20px"
            >
            <div node_ref=trace_warning_graph id=trace_warning_graph_id.clone()></div>
        </div>
    }
}
