use crate::graph_creation::GraphSeries;
use chrono::NaiveTime;
use leptos::prelude::*;
use leptos::{IntoView, component, view};

#[component]
pub fn Alerts() -> impl IntoView {
    // let (service_data_r, service_data_w) = signal_local::<
    //     Option<
    //         leptos::error::Result<Vec<api_structs::ui::series::SeriesWithData>, TrackedGlooError>,
    //     >,
    // >(None);
    // let _api_service_list_request_sender = LocalResource::new(move || {
    //     crate::dashboard::get_and_write_get_series_data_result(service_data_w)
    // });
    let mut x: Vec<String> = vec![];
    let mut y: Vec<f64> = vec![];
    let start = chrono::NaiveDate::from_ymd_opt(2025, 2, 17)
        .unwrap()
        .and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    let mut curr = start;
    for i in 0..50 {
        x.push(curr.format("%H:%M:%S").to_string());
        curr += chrono::Duration::minutes(5);
        if i % 2 == 0 {
            y.push(500.);
        } else {
            y.push(100.);
        }
    }
    let graph_series: Vec<GraphSeries> = vec![GraphSeries {
        name: "series 1".to_string(),
        x_values: x,
        y_values: y,
    }];
    let data = crate::graph_creation::GraphData {
        dom_id_to_render_to: "some".to_string(),
        y_name: "traces".to_string(),
        x_name: "minutes ago".to_string(),
        series: graph_series.clone(),
        click_event_timestamp_receiver: None,
    };
    let action = crate::graph_creation::create_create_chart_action();
    let (trace_warning_graph, trace_warning_graph_id) =
        crate::graph_creation::create_dom_el_ref_and_graph_call_action(data, action);
    view! {
         <div style="padding: 20px; color: white">
            <div id="triggered-alerts" style="resize: vertical; height: 150px; margin: 0 0 10px 0; padding: 10px; border: 2px solid white; border-radius: 10px; overflow: scroll;" >
                <div style="display: flex; margin-bottom: 5px">
                    <h3 style="display: inline; color: green; margin: 0 0 0 0">"Alerts: OK"</h3>
                </div>
                <table style="width: 100%; text-align: left">
                    <tr>
                        <th>"Alert Name"</th>
                        <th>"Generated at"</th>
                        <th>"Message"</th>
                        <th>"Sending"</th>
                    </tr>
                    <tr>
                        <td>"No Errors Any Service"</td>
                        <td>"1 min ago"</td>
                        <td>"Value 3 over 1"</td>
                        <td>"Delivered"</td>
                    </tr>
                    <tr>
                        <td>"No Warnings Any Service"</td>
                        <td>"5 min ago"</td>
                        <td>"Value 5 over threshold of 2"</td>
                        <td>"Connection refused trying to connect to http://slack.com/someweird. Invalid token wwadwwwwwwwwwweaaaaaaaaaadww"</td>
                    </tr>
                    <tr>
                        <td>"No Warnings Any Service"</td>
                        <td>"5 min ago"</td>
                        <td>"Value 5 over threshold of 2"</td>
                        <td>"Connection refused trying to connect to http://slack.com/someweird. Invalid token wwadwwwwwwwwwweaaaaaaaaaadww"</td>
                    </tr>
                    <tr>
                        <td>"No Warnings Any Service"</td>
                        <td>"5 min ago"</td>
                        <td>"Value 5 over threshold of 2"</td>
                        <td>"Connection refused trying to connect to http://slack.com/someweird. Invalid token wwadwwwwwwwwwweaaaaaaaaaadww"</td>
                    </tr>
                </table>
            </div>
            <div id="single-alert" style="padding: 10px; border: 2px solid white; border-radius: 10px" >
                <div style="margin-bottom: 10px; display: flex">
                    <input type="text" size="8" placeholder="alert name" />
                    <button style="margin-left: auto; color: red">"Delete"</button>
                    <button style="display:inline; margin-left: 10px">"Save"</button>
                </div>
                <div>
                    <h3 style="display: inline">"Filters: "</h3>
                    <input style="margin-left: 5px" type="text" size="150" value="" readonly>< /input>
                </div>
                <div id="request-charts">
                    <div style="margin-top: 10px">
                        <div style="margin-left: auto">
                            <p style="display: inline; margin: 0">"Alert Threshold"</p>
                            <input style="margin-left: 5px" type="text" size="4" placeholder="Min" />
                            <input style="margin-left: 5px" type="text" size="4" placeholder="Max" />
                            <p style="display: inline; margin: 0">" over "</p>
                            <select style="margin-bottom: 10px" id="time-range-selector">
                                <option value="5">"1 minutes"</option>
                                <option value="15">"5 minutes"</option>
                                <option value="60">"15 minutes"</option>
                                <option value="360">"1h"</option>
                            </select>
                        </div>
                    </div>
                    <div style="margin-top: 10px">
                        <select style="margin-bottom: 10px" id="time-range-selector">
                            <option value="5">"Past 5 minutes"</option>
                            <option value="15">"Past 15 minutes"</option>
                            <option value="60">"Past 1h"</option>
                            <option value="360">"Past 6h"</option>
                        </select>
                        <div style="height: 150px" node_ref=trace_warning_graph id=trace_warning_graph_id.clone()></div>
                    </div>
                </div>
            </div>
        </div>
    }
}
