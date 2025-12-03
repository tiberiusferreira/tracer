use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::info;
use gel_io_provider::{ToParameters, Transaction};
use gel_io_to_parameters::ToParameters;
use tracer::application_api::record_attribute;
use crate::api::handlers::instance::update::GelError;

pub mod database_old_traces_and_logs;
pub mod instance_runtime_data;
pub mod old_slack_notification;
pub async fn delete_old(tx: &mut Transaction) -> Result<(), GelError> {
    info!("Cleaning up old data");
    let cut_off_datetime = Utc::now() - chrono::Duration::days(3);
    // GelGen(query, out=Deleted, id=626aec)
    let query = "with cutoff := <datetime>$cutoff,
deleted_frags := (delete ReplayFragment
              filter .execution.last_seen_at < cutoff),
deleted_attrs := (delete ExecutionAttribute
              filter .execution.last_seen_at < cutoff),
deleted_execs := (delete Execution
              filter .last_seen_at < cutoff)
select {
  deleted_frags := count(deleted_frags),
  deleted_attrs := count(deleted_attrs),
  deleted_execs := count(deleted_execs)
}";

    // GelGen(in, id=626aec)
    #[derive(Clone, Serialize, Deserialize, ToParameters)]
    struct Args {
        cutoff: DateTime<Utc>,
    }
    // GelGen(out, id=626aec)
    #[derive(Clone, Serialize, Deserialize)]
    struct Deleted {
        deleted_frags: i64,
        deleted_attrs: i64,
        deleted_execs: i64,
    }

    let deleted: Deleted = tx.query_required_single(query, Args { cutoff: cut_off_datetime }.to_parameters()).await?;
    record_attribute("deleted_frags".to_string(), deleted.deleted_frags.to_string());
    record_attribute("deleted_attrs".to_string(), deleted.deleted_attrs.to_string());
    record_attribute("deleted_execs".to_string(), deleted.deleted_execs.to_string());
    info!("deleted {} replay fragments, {} execution attributes, {} executions", deleted.deleted_frags, deleted.deleted_attrs, deleted.deleted_execs);
    Ok(())
}
