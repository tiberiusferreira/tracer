use serde::{Deserialize, Serialize};
use uuid::Uuid;
use gel_io_provider::{ToParameters, Transaction};
use gel_io_to_parameters::ToParameters;
use crate::api::handlers::instance::update::GelError;

pub mod database_old_traces_and_logs;
pub mod instance_runtime_data;
pub mod old_slack_notification;
pub async fn delete_old(tx: &mut Transaction) -> Result<(), GelError> {
    tracing::info!("Cleaning up old data");
    delete_replay_frags(tx).await?;
    delete_execution_attrs(tx).await?;
    delete_executions(tx).await?;
    Ok(())
}

pub async fn delete_replay_frags(tx: &mut Transaction) -> Result<(), GelError> {
    tracing::info!("deleting replay fragments");
    // GelGen(query, out=Deleted, id=9a8742)
    let delete_replay_frags = "delete ReplayFragment filter .created_at < (datetime_current() - <cal::relative_duration>'1 month');";

    // GelGen(in, id=9a8742)
    #[derive(Clone, Serialize, Deserialize, ToParameters)]
    struct Args {}
    // GelGen(out, id=9a8742)
    #[derive(Clone, Serialize, Deserialize)]
    struct Deleted {
        id: Uuid,
    }
    let deleted: Vec<Deleted> = tx.query_multiple(delete_replay_frags, Args {}.to_parameters()).await?;
    tracing::info!("deleted {} replay fragments", deleted.len());
    Ok(())

    /*
    delete ReplayFragment filter .created_at < (datetime_current() - <cal::relative_duration>'1 month');
delete ExecutionAttribute filter .created_at < (datetime_current() - <cal::relative_duration>'1 month');
delete Execution filter .created_at < (datetime_current() - <cal::relative_duration>'1 month');
    */
}


pub async fn delete_execution_attrs(tx: &mut Transaction) -> Result<(), GelError> {
    // GelGen(query, out=Deleted, id=9a8743)
    let delete_exec_attrs = "delete ExecutionAttribute filter .created_at < (datetime_current() - <cal::relative_duration>'1 month');";

    // GelGen(in, id=9a8743)
    #[derive(Clone, Serialize, Deserialize, ToParameters)]
    struct Args {}
    // GelGen(out, id=9a8743)
    #[derive(Clone, Serialize, Deserialize)]
    struct Deleted {
        id: Uuid,
    }
    let deleted: Vec<Deleted> = tx.query_multiple(delete_exec_attrs, Args {}.to_parameters()).await?;
    tracing::info!("deleted {} execution attrs", deleted.len());
    Ok(())

    /*
    delete ReplayFragment filter .created_at < (datetime_current() - <cal::relative_duration>'1 month');
delete ExecutionAttribute filter .created_at < (datetime_current() - <cal::relative_duration>'1 month');
delete Execution filter .created_at < (datetime_current() - <cal::relative_duration>'1 month');
    */
}

pub async fn delete_executions(tx: &mut Transaction) -> Result<(), GelError> {
    // GelGen(query, out=Deleted, id=9a8744)
    let delete_exec_attrs = "delete Execution filter .created_at < (datetime_current() - <cal::relative_duration>'1 month');";

    // GelGen(in, id=9a8744)
    #[derive(Clone, Serialize, Deserialize, ToParameters)]
    struct Args {}
    // GelGen(out, id=9a8744)
    #[derive(Clone, Serialize, Deserialize)]
    struct Deleted {
        id: Uuid,
    }
    let deleted: Vec<Deleted> = tx.query_multiple(delete_exec_attrs, Args {}.to_parameters()).await?;
    tracing::info!("deleted {} executions", deleted.len());
    Ok(())
}