use crate::API_SERVER_URL_NO_TRAILING_SLASH;
use chrono::{Duration, NaiveDateTime};
use js_sys::Date;
use leptos::ev::{Event, MouseEvent};
use leptos::*;

use api_structs::{ApiTraceGridRow, Autocomplete, KeyValue, SearchFor};

#[derive(PartialEq, Clone, Debug)]
pub struct TraceGridRow {
    service_id: i64,
    id: i64,
    duration: Option<u64>,
    service_name: String,
    has_errors: bool,
    warning_count: u32,
    top_level_span_name: String,
    // sample_log: Option<String>,
    // key_value: Option<KeyValue>,
    // span: Option<String>,
    timestamp: u64,
}

#[derive(Debug, Clone)]
pub struct UserSearchInput {
    search_for: SearchFor,
}

impl Default for UserSearchInput {
    fn default() -> Self {
        let now = NaiveDateTime::from_timestamp_millis(Date::now().round() as i64).unwrap();
        Self {
            search_for: SearchFor {
                service_name: "".to_string(),
                top_level_span: "".to_string(),
                // span: "".to_string(),
                min_duration: 1000_000,
                max_duration: None,
                min_warns: 0,
                // key: "".to_string(),
                // value: "".to_string(),
                // event_name: "".to_string(),
                from_date_unix: u64::try_from(
                    (now - Duration::hours(1)).timestamp_nanos_opt().unwrap(),
                )
                .expect("timestamp to fit u64"),
                to_date_unix: u64::try_from(
                    (now + Duration::days(1)).timestamp_nanos_opt().unwrap(),
                )
                .expect("timestamp to fit u64"),
                only_errors: false,
            },
        }
    }
}

async fn get_grid_data(search_data: SearchFor, api_response_w: WriteSignal<Vec<TraceGridRow>>) {
    let url = format!("{}/api/traces-grid", API_SERVER_URL_NO_TRAILING_SLASH);
    log!("URL = {}", url);
    let resp: Vec<ApiTraceGridRow> = gloo_net::http::Request::post(&url)
        .json(&search_data)
        .unwrap()
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let trace_grid: Vec<TraceGridRow> = resp
        .into_iter()
        .map(|e| TraceGridRow {
            service_id: e.service_id,
            id: e.id,
            duration: e.duration_ns,
            service_name: e.service_name,
            has_errors: e.has_errors,
            warning_count: e.warning_count,
            top_level_span_name: e.top_level_span_name,
            // sample_log: e.event,
            // key_value: if let (Some(key), Some(value)) = (e.key, e.value) {
            //     Some(KeyValue {
            //         key,
            //         user_generated: true,
            //         value,
            //     })
            // } else {
            //     None
            // },
            // span: e.span,
            timestamp: e.timestamp,
        })
        .collect();
    api_response_w.set(trace_grid);
}

async fn get_autocomplete_data(search_data: SearchFor, api_response_w: WriteSignal<Autocomplete>) {
    let url = format!("{}/api/autocomplete-data", API_SERVER_URL_NO_TRAILING_SLASH);
    let resp: Autocomplete = gloo_net::http::Request::post(&url)
        .json(&search_data)
        .unwrap()
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    api_response_w.set(resp);
}

pub fn utc_to_local_date(utc: NaiveDateTime, offset_minutes: i64) -> NaiveDateTime {
    utc - Duration::minutes(offset_minutes)
}
pub fn local_date_to_utc(local: NaiveDateTime, offset_minutes: i64) -> NaiveDateTime {
    local + Duration::minutes(offset_minutes)
}

#[derive(Clone)]
enum RequestState {
    Idle,
    Running,
    RunningBehind,
}

fn debounced_api<S, T, Fu>(
    source: impl Fn() -> S + 'static,
    fetcher: impl Fn(S) -> Fu + 'static,
) -> ReadSignal<RequestState>
where
    S: PartialEq + Clone + 'static,
    T: 'static,
    Fu: std::future::Future<Output = T> + 'static,
{
    let (request_state_r, request_state_w) = create_signal(RequestState::Idle);
    let task_ref: StoredValue<Option<Resource<S, ()>>> = store_value(None);
    let api_request_sender = create_local_resource(source, {
        move |input: S| {
            let futt = fetcher(input);
            async move {
                if let RequestState::Running | RequestState::RunningBehind =
                    request_state_r.get_untracked()
                {
                    log!("Already running, setting as delayed");
                    request_state_w.set(RequestState::RunningBehind);
                    return;
                }
                log!("Was idle, setting as running");
                request_state_w.set(RequestState::Running);
                futt.await;
                let state_before = request_state_r.get_untracked();
                log!("Finished running");
                request_state_w.set(RequestState::Idle);
                if let Some(task) = task_ref.get_value() {
                    if let RequestState::RunningBehind = state_before {
                        log!("Was running behind, rerunning");
                        task.refetch();
                    }
                }
            }
        }
    });
    task_ref.set_value(Some(api_request_sender));
    request_state_r
}

#[component]
pub fn TraceGrid(root_path: String) -> impl IntoView {
    let (user_search_input_r, user_search_input_w) = create_signal(UserSearchInput::default());
    let (api_response_r, api_response_w) = create_signal(Vec::<TraceGridRow>::new());
    let (api_autocomplete_r, api_autocomplete_w) = create_signal(Autocomplete::default());
    let search_data: Memo<SearchFor> = create_memo(move |_prev: Option<&SearchFor>| {
        user_search_input_r.with(|v| v.search_for.clone())
    });
    let grid_request_state = debounced_api(
        move || search_data.get(),
        move |search_for| get_grid_data(search_for, api_response_w),
    );
    let autocomplete_request_state = debounced_api(
        move || search_data.get(),
        move |search_for| get_autocomplete_data(search_for, api_autocomplete_w),
    );
    let request_in_progress = Signal::derive(move || {
        match (grid_request_state.get(), autocomplete_request_state.get()) {
            (RequestState::Idle, RequestState::Idle) => false,
            (_, _) => true,
        }
    });
    let api_response_with_search_data =
        Signal::derive(move || (api_response_r.get(), user_search_input_r.get_untracked()));

    let service_name_changed = move |ev: Event| {
        let val = event_target_value(&ev);
        log!("Universal changed to: {}", val);
        user_search_input_w.update(|v| v.search_for.service_name = val);
    };

    let top_level_span_changed = move |ev: Event| {
        let val = event_target_value(&ev);
        log!("Top Level Span changed to: {}", val);
        user_search_input_w.update(|v| v.search_for.top_level_span = val);
    };

    let min_duration_changed = move |ev: Event| {
        let val = event_target_value(&ev);
        log!("Min Duration Value changed to: {}", val);
        if let Ok(val_ms) = val.parse::<u16>() {
            user_search_input_w.update(|v| v.search_for.min_duration = val_ms as u64 * 1000_000);
        } else if val.is_empty() {
            user_search_input_w.update(|v| v.search_for.min_duration = 0);
        } else {
            log!("Invalid Max Duration value");
            user_search_input_w.update(|_v| {});
        }
    };
    let max_duration_changed = move |ev: Event| {
        let val = event_target_value(&ev);
        log!("Max Duration Value changed to: {}", val);
        if let Ok(val_ms) = val.parse::<u16>() {
            user_search_input_w
                .update(|v| v.search_for.max_duration = Some(val_ms as u64 * 1000_000));
        } else if val.is_empty() {
            user_search_input_w.update(|v| v.search_for.max_duration = None);
        } else {
            log!("Invalid Max Duration value");
            user_search_input_w.update(|_v| {});
        }
    };
    let min_warns_changed = move |ev: Event| {
        let val = event_target_value(&ev);
        log!("Min Warns Value changed to: {}", val);
        if let Ok(min_warns) = val.parse::<u32>() {
            user_search_input_w.update(|v| v.search_for.min_warns = min_warns);
        } else if val.is_empty() {
            user_search_input_w.update(|v| v.search_for.min_warns = 0);
        } else {
            log!("Invalid Min Warns value");
            user_search_input_w.update(|_v| {});
        }
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
    let only_errors_checkbox_ref = create_node_ref::<leptos::html::Input>();
    let only_errors_changed = move |_click: MouseEvent| {
        user_search_input_w.update(|v| {
            v.search_for.only_errors = !v.search_for.only_errors;
            only_errors_checkbox_ref
                .get()
                .expect("only_errors checkbox to exist")
                .set_checked(v.search_for.only_errors);
        });
    };

    let tracer_counter = move || {
        let number_traces = api_response_r.get().len();
        let request_in_progress = request_in_progress.get();
        let text = if number_traces >= 100 {
            "99+ traces".to_string()
        } else {
            format!("{} traces", number_traces)
        };
        if request_in_progress {
            view! { <p style="margin: 0; background-color: yellow">{"Updating..."}</p>}
        } else {
            view! { <p style="margin: 0">{text}</p>}
        }
    };

    view! {
        <div class="main-grid">
            <div class="main">
                <TraceTable rows={api_response_with_search_data} root_path=root_path/>
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
                        let auto_complete_data = api_autocomplete_r.get();
                        let spans: Vec<_> = auto_complete_data.service_names.iter().map(|s|{
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
                <label class="search-panel__label">
                    "Top Level Span:"
                    <input on:input=top_level_span_changed
                        prop:value={move || user_search_input_r.with(|r| r.search_for.top_level_span.to_string())}
                        class="search-panel__input" type="text"  minlength="3" maxlength="50" size="20"
                        list="top-level-span-list"

                    />
                </label>
                {
                    move || {
                        let auto_complete_data = api_autocomplete_r.get();
                        let spans: Vec<_> = auto_complete_data.top_level_spans.iter().map(|s|{
                            view!{
                                <option value={s}></option>
                            }
                        }).collect();
                        view!{
                            <datalist id="top-level-span-list">
                              {spans}
                            </datalist>
                        }
                    }
                }
                <label class="search-panel__label">
                    "Duration:"
                    <div class="search-panel__input-flex-container">
                        <p>"From"</p>
                        <input on:input=min_duration_changed
                            prop:value={move || user_search_input_r.with(|r| (r.search_for.min_duration/1000_000).to_string())}
                            class="search-panel__input" type="text" maxlength="7" size="4"
                        />
                        <p>"to"</p>
                        <input on:input=max_duration_changed
                            prop:value={move || user_search_input_r.with(|r|
                                r.search_for.max_duration.map(|e| (e/1000_000).to_string()).unwrap_or("".to_string()))
                            }
                            class="search-panel__input" type="text" maxlength="7" size="4"
                        />
                        <p>"ms"</p>
                    </div>
                </label>
                <label class="search-panel__label">
                    "Errors Only:"
                    <input class="search-panel__input search-panel__input__inline" type="checkbox" checked=false
                        _ref=only_errors_checkbox_ref
                        on:click=only_errors_changed
                    />
                </label>
                <label class="search-panel__label">
                    "Min Warns:"
                    <input on:input=min_warns_changed
                            prop:value={move || user_search_input_r.with(|r| (r.search_for.min_warns).to_string())}
                            class="search-panel__input search-panel__input__inline" type="text" maxlength="5" size="2"
                        />
                </label>
            </div>
        </div>
    }
}
use leptos::logging::log;
use std::rc::Rc;

#[component]
pub fn DatePicker(
    label: String,
    date_to_display: Signal<NaiveDateTime>,
    on_change: Box<dyn Fn(NaiveDateTime) -> () + 'static>,
) -> impl IntoView {
    let on_change_rc = Rc::new(on_change);
    let on_change = Rc::clone(&on_change_rc);
    let on_date_changed_event = move |ev: Event| {
        let val = event_target_value(&ev);

        if let Ok(new_date) = NaiveDateTime::parse_from_str(&val, "%Y-%m-%dT%H:%M:%S") {
            on_change(new_date);
        } else {
            log!("Invalid date={}", val);
        }
    };
    let on_change = Rc::clone(&on_change_rc);
    let plus_button_clicked_event = move |_ev: MouseEvent| {
        let new_date = date_to_display.get() + Duration::hours(1);
        on_change(new_date);
    };
    let on_change = Rc::clone(&on_change_rc);
    let minus_button_clicked_event = move |_ev: MouseEvent| {
        let new_date = date_to_display.get() - Duration::hours(1);
        on_change(new_date);
    };
    view! {
        <label class="search-panel__label">
                {label}
                <div>
                    <button on:click=minus_button_clicked_event style="width: 3em; font-size: medium; margin: 5px;">"-1h"</button>
                    <button on:click=plus_button_clicked_event style="width: 3em; font-size: medium; margin: 5px;">"+1h"</button>
                </div>
                <input
                    on:change=on_date_changed_event
                    prop:value={move || {
                        let date = date_to_display.get().format("%Y-%m-%dT%H:%M:%S").to_string();
                        log!("Showing: {}", date);
                        date
                    }}
                    class="search-panel__input" type="datetime-local"
                />
        </label>
    }
}

fn highlight(original: String, term: String) -> Fragment {
    return if term.is_empty() {
        view! { <>{original}</>}
    } else {
        let o = original.to_lowercase();
        let Some((l, r)) = o.split_once(&term.to_lowercase()) else {
            return view! { <>{original}</>};
        };
        view! {
            <>
            {l.to_string()}
            <span style="color: red"> {term} </span>
            {r.to_string()}
            </>
        }
    };
}
#[component]
pub fn TraceTable(
    root_path: String,
    rows: Signal<(Vec<TraceGridRow>, UserSearchInput)>,
) -> impl IntoView {
    let headers = [
        view! {
            <th class="trace-table__cell">
                <a>"Service Name"</a>
            </th>
        },
        view! {
            <th class="trace-table__cell">
                <a>"Top Level Span"</a>
            </th>
        },
        view! {
            <th class="trace-table__cell">
                <a>"Duration (ms)"</a>
            </th>
        },
        // view! {
        //     <th class="trace-table__cell">
        //         <a>"Span"</a>
        //     </th>
        // },
        // view! {
        //     <th class="trace-table__cell">
        //         <a>"Log"</a>
        //     </th>
        // },
        // view! {
        //     <th class="trace-table__cell">
        //         <a>"KV"</a>
        //     </th>
        // },
        view! {
            <th class="trace-table__cell">
                <a>"Date"</a>
            </th>
        },
        view! {
            <th class="trace-table__cell">
                <a>"Warns"</a>
            </th>
        },
        view! {
            <th class="trace-table__cell">
                <a>"➔"</a>
            </th>
        },
    ];
    let html_headers = headers.to_vec();
    let html_rows = {
        move |rows: (Vec<TraceGridRow>, UserSearchInput)| {
            let user_search = rows.1;
            rows.0
                .into_iter()
                .map(|row| {
                    // let kv = row.key_value.map(|kv| format!("{} => {}", kv.key, kv.value));
                    let offset_minutes = js_sys::Date::new_0().get_timezone_offset() as i64;
                    let row_container_class = if row.has_errors{
                        "row-container row-container__error".to_string()
                    }else{
                        "row-container".to_string()
                    };
                    let node = view! {

                <tr class={row_container_class}>
                        <td class="trace-table__cell">{highlight( row.service_name.clone(), user_search.search_for.service_name.clone())}</td>
                        <td class="trace-table__cell">{row.top_level_span_name.to_string()}</td>
                        <td class="trace-table__cell">{(row.duration.map(|e| (e/1000_000).to_string())).unwrap_or_default()}</td>
                        // <td class="trace-table__cell">{highlight( row.span.unwrap_or_default(), user_search.search_for.span.clone())}</td>
                        // <td class="trace-table__cell">{row.sample_log.map(|sl| highlight( sl, user_search.search_for.event_name.clone()))}</td>
                        // <td class="trace-table__cell">{highlight( kv.unwrap_or_default(), user_search.search_for.key.clone())}</td>
                        <td class="trace-table__cell">
                            {
                                // printable_local_date()
                                // let local_date_str = 
                                crate::printable_local_date(row.timestamp)
                                // let timestamp = api_structs::time_conversion::nanos_to_db_i64(row.timestamp);
                                // let nanos_in_1_sec = 1_000_000_000;
                                // let timestamp =chrono::NaiveDateTime::from_timestamp_opt(timestamp/nanos_in_1_sec, u32::try_from(timestamp%nanos_in_1_sec).unwrap()).unwrap();
                                // utc_to_local_date(timestamp, offset_minutes).format("%Y-%m-%d %H:%M:%S").to_string()
                            }
                        </td>
                        <td class="trace-table__cell">{row.warning_count}</td>
                        <td class="trace-table__cell">
                            <a href={format!("{}trace/?service_id={}&trace_id={}&start_timestamp={}", root_path, row.service_id, row.id, row.timestamp)}>{"➔"}</a>
                        </td>
                </tr>
            };
                    node
                })
                .collect::<Vec<HtmlElement<_>>>()
        }
    };
    view! {
        <table class="trace-table">
            <tr class="row-container">
                    {html_headers}
            </tr>
            {move || html_rows(rows.get())}
        </table>
    }
}
