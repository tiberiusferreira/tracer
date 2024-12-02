use crate::api::state::AppState;
use crate::api::ApiError;
use axum::extract::State;
use axum::Json;
use tracing::instrument;

#[instrument(skip_all, err(Debug))]
pub async fn get_all_series(
    State(app_state): State<AppState>,
) -> Result<Json<Vec<api_structs::ui::series::SeriesWithData>>, ApiError> {
    let con = app_state.con;
    let all_series = crate::series::database::get_all_series(con.clone()).await?;
    let mut series_with_data = vec![];
    for s in all_series {
        let data = crate::series::database::get_series_data(con.clone(), &s).await?;
        let data = data
            .into_iter()
            .map(|d| api_structs::ui::series::SeriesDataPoint {
                time: d.time,
                data: d.data,
                series_id: d.series_id,
            })
            .collect();
        series_with_data.push(api_structs::ui::series::SeriesWithData {
            id: s.id,
            name: s.name,
            sql: s.sql,
            check_window: s.check_window,
            max_missing_data_points: s.max_missing_data_points,
            max_value_threshold: s.max_value_threshold,
            min_value_threshold: s.min_value_threshold,
            data,
            alert_checks: s
                .alert_checks
                .into_iter()
                .map(|alert_check| api_structs::ui::series::AlertCheck {
                    result: match alert_check.alert_message {
                        None => api_structs::ui::series::AlertCheckResult::Ok,
                        Some(alert_message) => {
                            api_structs::ui::series::AlertCheckResult::AlertMessage(alert_message)
                        }
                    },
                    checked_at: alert_check.created_at,
                })
                .collect(),
        });
    }
    Ok(Json(series_with_data))
}
