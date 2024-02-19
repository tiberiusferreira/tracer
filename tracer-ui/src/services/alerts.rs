use api_structs::ui::service::alerts::AlertConfig;
use leptos::html::Div;
use leptos::view;

pub fn alerts_html(alert_config: AlertConfig) -> leptos::HtmlElement<Div> {
    let AlertConfig {
        service_wide,
        instance_wide,
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
            <p style="text-align: center">{"Graph Alerts:"}</p>
                <div style="display: flex; flex-wrap: wrap; gap: 20px; justify-content: center">
                    <div style="">
                        <label style="align-self: center" for="filters">"Min Instance Count: "</label>
                        <input type="text"  name="filters" size="6" value={service_wide.min_instance_count} />
                        <button style="margin-left: 5px; margin-bottom: 10px" disabled=true>"Apply"</button>
                    </div>
                    <div style="">
                        <label style="align-self: center" for="filters">"Max Active Traces: "</label>
                        <input type="text"  name="filters" size="6" value={service_wide.max_active_traces} />
                        <button style="margin-left: 5px; margin-bottom: 10px" disabled=true>"Apply"</button>
                    </div>
                    <div style="">
                        <label style="align-self: center" for="filters">"Max SpE Export Buffer Usage: "</label>
                        <input type="text"  name="filters" size="6" value={instance_wide.max_export_buffer_usage} />
                        <button style="margin-left: 5px; margin-bottom: 10px" disabled=true>"Apply"</button>
                    </div>
                    <div style="">
                        <label style="align-self: center" for="filters">"Max Orphan Events/min: "</label>
                        <input type="text"  name="filters" size="6" value={instance_wide.orphan_events_per_minute_usage} />
                        <button style="margin-left: 5px; margin-bottom: 10px" disabled=true>"Apply"</button>
                    </div>
                    <div style="">
                        <label style="align-self: center" for="filters">"Max Orphan Events Dropped By Samping/min: "</label>
                        <input type="text"  name="filters" size="6" value={instance_wide.max_orphan_events_dropped_by_sampling_per_min} />
                        <button style="margin-left: 5px; margin-bottom: 10px" disabled=true>"Apply"</button>
                    </div>
                    <div style="">
                        <label style="align-self: center" for="filters">"Max SpE Dropped due to Full Export Buffer/min: "</label>
                        <input type="text" name="filters" size="6" value={instance_wide.max_spe_dropped_due_to_full_export_buffer_per_min} />
                        <button style="margin-left: 5px; margin-bottom: 10px" disabled=true>"Apply"</button>
                    </div>
                    <div style="">
                        <label style="align-self: center" for="filters">"Max Received SpE: "</label>
                        <input type="text" name="filters" size="6" value={instance_wide.max_received_spe} />
                        <button style="margin-left: 5px; margin-bottom: 10px" disabled=true>"Apply"</button>
                    </div>
                    <div style="">
                        <label style="align-self: center" for="filters">"Max Received Trace kbs: "</label>
                        <input type="text" name="filters" size="6" value={instance_wide.max_received_trace_kb} />
                        <button style="margin-left: 5px; margin-bottom: 10px" disabled=true>"Apply"</button>
                    </div>
                    <div style="">
                        <label style="align-self: center" for="filters">"Max Received Orphan Event kbs: "</label>
                        <input type="text" name="filters" size="6" value={instance_wide.max_received_orphan_event_kb} />
                        <button style="margin-left: 5px; margin-bottom: 10px" disabled=true>"Apply"</button>
                    </div>
                    <div style="">
                        <label style="align-self: center" for="filters">"Percentage Check Time Window sec: "</label>
                        <input type="text" name="filters" size="6" value={trace_wide.percentage_check_time_window_secs} />
                        <button style="margin-left: 5px; margin-bottom: 10px" disabled=true>"Apply"</button>
                    </div>
                    <div style="">
                        <label style="align-self: center" for="filters">"Percentage Check Min Number Samples: "</label>
                        <input type="text" name="filters" size="6" value={trace_wide.percentage_check_min_number_samples} />
                        <button style="margin-left: 5px; margin-bottom: 10px" disabled=true>"Apply"</button>
                    </div>

                </div>
                <p style="text-align: center">{"Per Trace Global Alerts:"}</p>
                <div style="display: flex; flex-wrap: wrap; gap: 20px; justify-content: center">
                    <div style="">
                        <label style="align-self: center" for="filters">"Max Duration (ms): "</label>
                        <input type="text" name="filters" size="6" value={trace_wide.max_trace_duration_ms} />
                        <button style="margin-left: 5px; margin-bottom: 10px" disabled=true>"Apply"</button>
                    </div>
                    <div style="">
                        <label style="align-self: center" for="filters">"Warning %: "</label>
                        <input type="text" name="filters" size="6" value={trace_wide.max_traces_with_warning_percentage} />
                        <button style="margin-left: 5px; margin-bottom: 10px" disabled=true>"Apply"</button>
                    </div>
                    <div style="">
                        <label style="align-self: center" for="filters">"Max Dropped by Sampling/min: "</label>
                        <input type="text" name="filters" size="6" value={trace_wide.max_traces_dropped_by_sampling_per_min} />
                        <button style="margin-left: 5px; margin-bottom: 10px" disabled=true>"Apply"</button>
                    </div>
                </div>

                <p style="text-align: center">{"Per Trace Alert overwrites:"}</p>
                <div style="display: flex; flex-wrap: wrap; gap: 20px; justify-content: center">
                    <div style="">
                        <label style="align-self: center" for="trace_name">"Trace Name:"</label>
                        <input style="margin-right: 7px" type="text" name="trace_name" size="15" />
                        <label style="align-self: center" for="trace_duration">"Duration (ms):"</label>
                        <input style="margin-right: 7px" type="text" name="trace_duration" size="3" />
                        <label style="align-self: center" for="warning_percentage">"Warning %:"</label>
                        <input style="margin-right: 7px" type="text" name="warning_percentage" size="3" />
                        <button style="margin-left: 5px; margin-bottom: 10px" disabled=true>"Apply"</button>
                    </div>
                </div>

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
