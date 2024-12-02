use crate::series::{Series, SeriesDataPoint};
use sqlx::PgPool;
use tracked_error::{SerdeJsonError, SqlxError};

pub async fn get_all_series(con: PgPool) -> Result<Vec<Series>, crate::error::SqlxOrSerdeJson> {
    let series: Vec<serde_json::Value> = sqlx::query_scalar!(
        "select
       json_object(
           'id': series.id,
           'name': name,
           'sql': sql,
           'check_window': check_window,
           'max_missing_data_points': max_missing_data_points,
           'max_value_threshold': max_value_threshold,
           'min_value_threshold': min_value_threshold,
           'alert_checks': json_agg(
                   json_object(
                           'id' : series_alert_check.id,
                           'alert_message' : alert_message,
                           'notification_sent' : notification_sent,
                           'created_at' : series_alert_check.created_at
                   )
           )
       ) as \"value!\"
from series
         left join series_alert_check on series_alert_check.series_id = series.id
group by series.id;"
    )
    .fetch_all(&con)
    .await
    .map_err(SqlxError::from)?;
    let series = series
        .into_iter()
        .map(|e| {
            let val_as_str = e.to_string();
            serde_json::from_value(e)
                .map_err(|err| SerdeJsonError::from_serde_json_error(err, val_as_str))
        })
        .collect::<Result<Vec<Series>, SerdeJsonError>>();
    Ok(series?)
}

pub async fn get_series_data(
    con: PgPool,
    single_series: &Series,
) -> Result<Vec<SeriesDataPoint>, SqlxError> {
    let series_data: Vec<SeriesDataPoint> =
        sqlx::query_as(&single_series.sql).fetch_all(&con).await?;
    Ok(series_data)
}
