use crate::series::SeriesDataPoint;
use chrono::{NaiveDateTime, Utc};
use sqlx::PgPool;
use tracing::{debug, error, info, instrument};
use tracked_error::{error_chain_to_pretty_formatted, SqlxError};

#[instrument(skip_all)]
pub async fn execute_series_and_check_for_alerts(
    con: PgPool,
) -> Result<(), crate::error::SqlxOrSerdeJson> {
    let series = crate::series::database::get_all_series(con.clone()).await?;
    for single_series in &series {
        info!(
            id = single_series.id,
            name = single_series.name,
            "processing single_series"
        );
        debug!(?single_series);
        let series_data: Vec<SeriesDataPoint> =
            match crate::series::database::get_series_data(con.clone(), single_series).await {
                Ok(series_data) => series_data,
                Err(error) => {
                    let err = error_chain_to_pretty_formatted(error);
                    let name = &single_series.name;
                    error!("Problem running query for series {name}:\n{err}");
                    continue;
                }
            };
        let check_window = single_series.check_window;
        let mut missing_count = 0;
        let mut first_alert: Option<String> = None;
        for data_point in series_data.into_iter().take(check_window as usize) {
            match (data_point.data, data_point.series_id) {
                (Some(data), Some(series_id)) => {
                    if let Some(max_value_threshold) = single_series.max_value_threshold {
                        if data > max_value_threshold as i64 {
                            let name = &single_series.name;
                            first_alert=Some(format!("Series {name} has value ({data}) over max threshold ({max_value_threshold})"));
                            break;
                        }
                    }
                    if let Some(min_value_threshold) = single_series.min_value_threshold {
                        if data < min_value_threshold as i64 {
                            let name = &single_series.name;
                            first_alert=Some(format!("Series {name} has value ({data}) under min threshold ({min_value_threshold})"));
                            break;
                        }
                    }
                }
                _ => {
                    missing_count += 1;
                    if let Some(max_missing_data_points) = single_series.max_missing_data_points {
                        if missing_count > max_missing_data_points {
                            let name = &single_series.name;
                            first_alert=Some(format!("Series {name} has more missing values ({missing_count}) than allowed ({max_missing_data_points})"));
                            break;
                        }
                    }
                }
            }
        }

        info!(first_alert);
        let res = sqlx::query!(
            "insert into series_alert_check (series_id, alert_message) values ($1, $2);",
            single_series.id,
            first_alert
        )
        .execute(&con)
        .await
        .map_err(SqlxError::from)?;
        info!("inserted series_alert_check");
    }
    Ok(())
}
