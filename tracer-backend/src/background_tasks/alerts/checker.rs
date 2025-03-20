// use crate::error::EdgeDBOrSerdeJson;
// use chrono::{Duration, Utc};
// use edgedb_codegen::edgedb_query;
// use edgedb_derive::Queryable;
// use edgedb_protocol::model::Datetime;
// use edgedb_tokio::Client;
// use serde::{Deserialize, Serialize};
// use tracing::{info, instrument, trace};
// use tracked_error::EdgeDBError;
//
// edgedb_query!(
//     get_time_series,
//     "select TimeSeries{
//   name,
//   look_back_window_seconds,
//   max_interval_without_data_seconds,
//   max_value_threshold,
//   min_value_threshold,
//   query
// }"
// );
//
// edgedb_query!(
//     insert_check_result,
//     "insert TimeSeriesAlertChecks{
//   time_series := (
//     assert_single(assert_exists(
//       (select TimeSeries filter .name = <str>$time_series_name)
//     )
//   )),
//   alert_message := <optional str>$alert_message,
// }"
// );
//
// #[derive(Queryable, Debug, Clone, Serialize, Deserialize)]
// struct SingleSeriesData {
//     series_name: String,
//     data_points: Vec<DataPoint>,
// }
//
// #[derive(Queryable, Debug, Clone, Serialize, Deserialize)]
// struct DataPoint {
//     date: chrono::DateTime<Utc>,
//     value: i64,
// }
//
// fn check_single_series_for_issues(
//     data: &SingleSeriesData,
//     _min_threshold: Option<i32>,
//     _max_threshold: Option<i32>,
//     max_interval_without_data_seconds: Option<i32>,
// ) -> Result<(), String> {
//     info!(data.series_name, "checking series for issues");
//     let mut data_sorted = data.clone();
//     data_sorted.data_points.sort_by_key(|e| e.date);
//     let mut last_point: Option<DataPoint> = None;
//     for data_point in data_sorted.data_points {
//         if let (Some(last_point), Some(max_interval_without_data_seconds)) =
//             (&last_point, max_interval_without_data_seconds)
//         {
//             let elapsed_as_seconds = (data_point.date - last_point.date).num_seconds();
//             if (max_interval_without_data_seconds as i64) < elapsed_as_seconds {
//                 return Err(format!(
//                     "Interval between data points ({elapsed_as_seconds}s) is \
// greater than allowed ({max_interval_without_data_seconds})s for dates {} and {}",
//                     last_point.date, data_point.date
//                 ));
//             }
//         }
//         last_point = Some(data_point);
//     }
//     Ok(())
// }
// #[instrument(skip_all)]
// pub async fn execute_series_and_check_for_alerts(con: Client) -> Result<(), EdgeDBOrSerdeJson> {
//     let time_series = get_time_series::query(&con)
//         .await
//         .map_err(|e| EdgeDBOrSerdeJson::EdgeDB(EdgeDBError::from(e)))?;
//     for single_series in &time_series {
//         info!(name = single_series.name, "processing single_series");
//         info!(query = single_series.query, "running series query");
//         let end = Utc::now();
//         let start = end - Duration::seconds(single_series.look_back_window_seconds as i64);
//         let start = edgedb_protocol::value::Value::Datetime(
//             Datetime::try_from(start).expect("to have valid datetime"),
//         );
//         let end = edgedb_protocol::value::Value::Datetime(
//             Datetime::try_from(end).expect("to have valid datetime"),
//         );
//         let query_params = edgedb_protocol::named_args! {
//             "start_datetime" => start,
//             "end_datetime" => end
//         };
//         let series_data: Vec<SingleSeriesData> = con
//             .query(&single_series.query, &query_params)
//             .await
//             .map_err(|e| EdgeDBOrSerdeJson::EdgeDB(EdgeDBError::from(e)))?;
//         for single_series_data in &series_data {
//             info!(series.name = single_series_data.series_name, "series data");
//             trace!(single_series = ?single_series_data, "full series data");
//             let check_result = check_single_series_for_issues(
//                 single_series_data,
//                 single_series.min_value_threshold,
//                 single_series.max_interval_without_data_seconds,
//                 single_series.max_value_threshold,
//             );
//             info!(series.check_result = ?check_result, "series check result");
//             let error = check_result.err();
//             info!("inserting result");
//             insert_check_result::query(
//                 &con,
//                 &insert_check_result::Input {
//                     time_series_name: single_series.name.clone(),
//                     alert_message: error,
//                 },
//             )
//             .await
//             .map_err(|e| EdgeDBOrSerdeJson::EdgeDB(EdgeDBError::from(e)))?;
//             info!("inserted");
//         }
//     }
//     Ok(())
// }
