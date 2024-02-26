use api_structs::ui::service::alerts::AlertConfig;
use leptos::html::Div;
use leptos::view;

pub fn alerts_html(alert_config: AlertConfig) -> leptos::HtmlElement<Div> {
    let AlertConfig {
        service_wide,
        trace_wide,
        service_alert_config_trace_overwrite,
    } = alert_config;
    let mut overwrite_rows = vec![];
    for (trace_name, trace_alert_config) in service_alert_config_trace_overwrite {
        let row = view! {
             <tr class="row-container">
                  <th class="trace-table__cell">
                      {trace_name}
                  </th>
                  <th class="trace-table__cell">
                      {trace_alert_config.max_trace_duration_ms}
                  </th>
                  <th class="trace-table__cell">
                      {trace_alert_config.max_traces_with_warning_percentage}
                  </th>

              </tr>
        };
        overwrite_rows.push(row);
    }

    view! {
        <div>
            <p style="text-align: center">{"Alerts:"}</p>
                <div style="display: flex; flex-wrap: wrap; gap: 20px; justify-content: center">
                    <p style="margin: 0px">{"Min Instance Count: "}<b>{service_wide.min_instance_count}</b></p>
                    <p style="margin: 0px">{"Max Active Traces: "}<b>{service_wide.max_active_traces_count}</b></p>
                    <p style="margin: 0px">{"Max Export Buffer Usage: "}<b>{service_wide.max_export_buffer_usage_percentage}</b>"%"</p>
                </div>
                <p style="text-align: center">{"Per Trace Global Alerts:"}</p>
                <div style="display: flex; flex-wrap: wrap; gap: 20px; justify-content: center">
                    <p style="margin: 0px">{"Percentage Check Time Window sec: "}<b>{trace_wide.percentage_check_time_window_secs}</b></p>
                    <p style="margin: 0px">{"Percentage Check Min Samples: "}<b>{trace_wide.percentage_check_min_number_samples}</b></p>
                    <p style="margin: 0px">{"Max Duration: "}<b>{trace_wide.max_trace_duration_ms}</b>"ms"</p>
                    <p style="margin: 0px">{"Max Warning: "}<b>{trace_wide.max_traces_with_warning_percentage}</b>"%"</p>
                </div>
                <div style="height: 20px"></div>
                <table class="trace-table">
                    <tr class="row-container">
                        <th style="text-align: center" colspan="4" class="trace-table__cell">
                            <a>"Trace Alert Overrides"</a>
                        </th>
                    </tr>
                    <tr class="row-container">
                        <th class="trace-table__cell">
                            <a>"Trace Name"</a>
                        </th>
                        <th class="trace-table__cell">
                            <a>"Allowed Duration (ms)"</a>
                        </th>
                        <th class="trace-table__cell">
                            <a>"Allowed Warning %"</a>
                        </th>
                    </tr>
                    {overwrite_rows}
                </table>
        </div>
    }
}
