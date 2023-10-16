use crate::grid::DatePicker;
use crate::grid::{local_date_to_utc, utc_to_local_date};
use crate::API_SERVER_URL_NO_TRAILING_SLASH;
use api_structs::exporter::{Log, NewFiltersRequest, ServiceLogRequest, ServiceNameList, Severity};
use chrono::{Duration, NaiveDateTime};
use js_sys::Date;
use leptos::ev::{Event, MouseEvent};
use leptos::logging::log;
use leptos::*;
use leptos::{component, SignalGet, SignalSet, WriteSignal};
#[derive(Debug, Clone, PartialEq)]
pub struct UserSearchInput {
    search_for: ServiceLogRequest,
}

impl Default for UserSearchInput {
    fn default() -> Self {
        let now = NaiveDateTime::from_timestamp_millis(Date::now().round() as i64).unwrap();
        Self {
            search_for: ServiceLogRequest {
                service_name: "".to_string(),
                from_date_unix: u64::try_from(
                    (now - Duration::hours(1)).timestamp_nanos_opt().unwrap(),
                )
                .expect("timestamp to fit u64"),
                to_date_unix: u64::try_from(
                    (now + Duration::days(1)).timestamp_nanos_opt().unwrap(),
                )
                .expect("timestamp to fit u64"),
            },
        }
    }
}
#[component]
pub fn Logs(root_path: String) -> impl IntoView {
    let (user_search_input_r, user_search_input_w) = create_signal(UserSearchInput::default());
    let (service_name_list_r, service_name_list_w) = create_signal(Option::<ServiceNameList>::None);
    let (logs_r, logs_w) = leptos::create_signal(Option::<Vec<Log>>::None);
    let _api_service_names_request =
        create_local_resource(move || (), move |_| get_service_list(service_name_list_w));
    let _api_service_logs_request = create_local_resource(
        move || user_search_input_r.get(),
        move |user_search_input| get_logs(logs_w, user_search_input),
    );

    let logs_view = move || {
        let logs = logs_r.get();
        match logs {
            None => {
                view! {
                    <div style="padding: 20px; color: white">
                       <p>"Loading, maybe failed, check logs"</p>
                    </div>
                }
            }
            Some(logs) => {
                let logs_view = logs.iter().map(|l|{
                    let date = crate::printable_local_date_ms(l.timestamp);
                    let event_msg = format!("{date} - {}", l.value);
                    let color = match l.severity{
                        Severity::Warn => {
                            "yellow"
                        }
                        Severity::Error => {
                            "red"
                        }
                        _ => {
                            "white"
                        }
                    };
                    view!{
                        <>
                            <div style="width: 100%; background-color: rgba(255,255,255,0.05)">
                                <p class="trace-details__event" style={format!("color: {color}")}>{event_msg}</p>
                            </div>
                        </>
                    }
                }).collect_view();
                view! {
                    <div style="padding: 20px; color: white">
                       {logs_view}
                    </div>
                }
            }
        }
    };

    let current_from_datetime = Signal::derive(move || {
        let offset = js_sys::Date::new_0().get_timezone_offset() as i64;

        user_search_input_r.with(|r| {
            let timestamp =
                i64::try_from(r.search_for.from_date_unix).expect("timestamp to fit i64");
            utc_to_local_date(
                NaiveDateTime::from_timestamp_opt(
                    timestamp / 1_000_000_000,
                    (timestamp % 1_000_000_000) as u32,
                )
                .unwrap(),
                offset,
            )
        })
    });

    let service_name_changed = move |ev: Event| {
        let val = event_target_value(&ev);
        log!("Universal changed to: {}", val);
        user_search_input_w.update(|v| v.search_for.service_name = val);
    };

    let current_to_datetime = Signal::derive(move || {
        let offset = js_sys::Date::new_0().get_timezone_offset() as i64;
        user_search_input_r.with(|r| {
            let timestamp = i64::try_from(r.search_for.to_date_unix).expect("timestamp to fit i64");
            utc_to_local_date(
                NaiveDateTime::from_timestamp_opt(
                    timestamp / 1_000_000_000,
                    (timestamp % 1_000_000_000) as u32,
                )
                .unwrap(),
                offset,
            )
        })
    });

    let tracer_counter = move || {
        let number_logs = logs_r.get().unwrap_or_default().len();
        // let request_in_progress = request_in_progress.get();
        // let text = if number_logs >= 100 {
        //     "99+ logs".to_string()
        // } else {
        let text = format!("{} logs", number_logs);
        // };
        // if request_in_progress {
        //     view! { <p style="margin: 0; background-color: yellow">{"Updating..."}</p>}
        // } else {
        view! { <p style="margin: 0">{text}</p>}
        // }
    };

    let from_changed = move |new_datetime: NaiveDateTime| {
        user_search_input_w.update(|v| {
            let offset_minutes = js_sys::Date::new_0().get_timezone_offset() as i64;
            if let Ok(timestamp_nanos) = u64::try_from(
                local_date_to_utc(new_datetime, offset_minutes)
                    .timestamp_nanos_opt()
                    .unwrap(),
            ) {
                v.search_for.from_date_unix = timestamp_nanos;
            } else {
                log!("From date out of bounds!")
            }
        });
    };
    let to_changed = move |new_datetime: NaiveDateTime| {
        let offset_minutes = js_sys::Date::new_0().get_timezone_offset() as i64;
        user_search_input_w.update(|v| {
            if let Ok(timestamp_nanos) = u64::try_from(
                local_date_to_utc(new_datetime, offset_minutes)
                    .timestamp_nanos_opt()
                    .unwrap(),
            ) {
                v.search_for.to_date_unix = timestamp_nanos;
            } else {
                log!("From date out of bounds!")
            }
        });
    };

    view! {
        <div class="main-grid">
            <div class="main">
                {logs_view}
            </div>
            <div class="search-panel">
                    <h1 class="traces-counter">{tracer_counter}</h1>
                    <DatePicker
                        label="From (local):".to_string()
                        date_to_display=current_from_datetime
                        on_change=Box::new(from_changed)
                    />
                    <DatePicker
                        label="To (local):".to_string()
                        date_to_display=current_to_datetime
                        on_change=Box::new(to_changed)
                    />
                    <label class="search-panel__label">
                        "Service Name:"
                        <input on:input=service_name_changed
                            prop:value={move || user_search_input_r.with(|r| r.search_for.service_name.to_string())}
                            class="search-panel__input" type="text"  minlength="3" maxlength="50" size="20"
                            list="service-name-list"
                        />
                    </label>
                    {
                        move || {
                            let service_name_list = service_name_list_r.get().unwrap_or_default();
                            let spans: Vec<_> = service_name_list.iter().map(|s|{
                                view!{
                                    <option value={s}></option>
                                }
                            }).collect();
                            view!{
                                <datalist id="service-name-list">
                                  {spans}
                                </datalist>
                            }
                        }
                    }
            </div>
        </div>
        // {view}
    }
}

async fn update_filter(new_filter: NewFiltersRequest) -> Result<(), String> {
    log!("Sending req");
    log!(
        "Updating instance {} to {}",
        new_filter.instance_id,
        new_filter.filters
    );
    let traces = gloo_net::http::Request::post(&format!(
        "{}/api/instances/filter",
        API_SERVER_URL_NO_TRAILING_SLASH
    ))
    .json(&new_filter)
    .unwrap()
    .send()
    .await
    .unwrap()
    .status();
    match traces {
        200 => Ok(()),
        x => Err(format!("Bad status back: {}", x)),
    }
}

async fn get_service_list(w: WriteSignal<Option<ServiceNameList>>) {
    log!("Sending get_service_list request");
    let service_list: ServiceNameList = gloo_net::http::Request::get(&format!(
        "{}/api/logs/service_names",
        API_SERVER_URL_NO_TRAILING_SLASH
    ))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    log!("Got logs service list back: {:?}", service_list);
    w.set(Some(service_list));
}

async fn get_logs(w: WriteSignal<Option<Vec<Log>>>, user_search_input: UserSearchInput) {
    log!("Log search: {:#?}", user_search_input);
    let logs: Vec<Log> =
        gloo_net::http::Request::get(&format!("{}/api/logs", API_SERVER_URL_NO_TRAILING_SLASH))
            .query([
                ("service_name", user_search_input.search_for.service_name),
                (
                    "from_date_unix",
                    user_search_input.search_for.from_date_unix.to_string(),
                ),
                (
                    "to_date_unix",
                    user_search_input.search_for.to_date_unix.to_string(),
                ),
            ])
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
    log!("Got logs back: {logs:#?}");
    w.set(Some(logs));
}