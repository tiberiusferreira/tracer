use sqlx::PgPool;
use tracing::instrument;

#[instrument(skip_all)]
pub async fn delete_old_traces_logging_error(_con: &PgPool) {
    //     let res: PgQueryResult = match sqlx::query!(
    //         "delete
    // from trace
    //     using trace_cache
    // where trace_cache.instance_id = trace.instance_id
    //   and trace_cache.trace_id = trace.id
    //   and trace_cache.timestamp < (now() - INTERVAL '10 DAY');"
    //     )
    //     .execute(con)
    //     .instrument(info_span!("deleting_old_traces"))
    //     .await
    //     {
    //         Ok(res) => res,
    //         Err(err) => {
    //             let err_str = error_chain_to_pretty_formatted(SqlxError::from_sqlx_error(
    //                 err,
    //                 "deleting old traces",
    //             ));
    //             error!("{err_str}");
    //             return;
    //         }
    //     };
    //     info!("Deleted {} records", res.rows_affected());
    unimplemented!()
}

#[instrument(skip_all)]
pub async fn delete_old_orphan_events_logging_error(_con: &PgPool) {
    // let res: PgQueryResult =
    //     match sqlx::query!("delete from orphan_event where timestamp < (EXTRACT(epoch FROM now() - INTERVAL '10 DAY') * 1000000000);")
    //         .execute(con)
    //         .instrument(info_span!("deleting_old_orphan_events"))
    //         .await {
    //         Ok(res) => { res }
    //         Err(err) => {
    //             let err_str = error_chain_to_pretty_formatted(SqlxError::from_sqlx_error(err, "deleting old orphan events"));
    //             error!("{err_str}");
    //             return;
    //         }
    //     };
    // info!("Deleted {} records", res.rows_affected());
    unimplemented!()
}
