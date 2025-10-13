use crate::api::handlers::instance::update::{InstanceServiceInformation, ProcessUpdateError};
use api_structs::instance::update::ExecutionRecordingSnapshot;
use gel_io_provider::{Parameter, ToParameters, Transaction};
use std::collections::{HashSet};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use tracing::trace;
use uuid::Uuid;
use gel_io_to_parameters::ToParameters;

pub async fn get_instance_service_information(
    tx: &mut Transaction,
    instance_id: Uuid,
) -> Result<Option<InstanceServiceInformation>, ProcessUpdateError> {
    let instance_service_info: Option<InstanceServiceInformation> = tx
        .query_optional(
            "select ServiceInstance{
  service_name := .service.name,
  service_env := .service.env,
  instance_id := .id,
} filter .id=<uuid>$instance_id",
            IndexMap::from([("instance_id".to_string(), Parameter::from(instance_id))]),
        )
        .await?;
    Ok(instance_service_info)
}

pub async fn update_instance_profile(
    tx: &mut Transaction,
    instance_id: Uuid,
    profile: &str,
) -> Result<(), super::GelError> {
    // GelGen(query, out=InsertOut, id=6fd5f6)
    let q = "update ServiceInstance
  filter .id = <uuid>$id
  set {
    latest_profile_base64 := <str>$profile_base64
  };";

    // GelGen(in, id=6fd5f6)
    #[derive(Clone, Serialize, Deserialize, ToParameters)]
    struct Args {
        profile_base64: String,
        id: Uuid,
    }
    // GelGen(out, id=6fd5f6)
    #[derive(Clone, Debug, Serialize, Deserialize)]
    struct InsertOut {
        id: Uuid,
    }
    let inserted: Vec<InsertOut> = tx.query_multiple(q, Args {
        profile_base64: profile.to_string(),
        id: instance_id,
    }.to_parameters()).await?;
    trace!("inserted: {:?}", inserted);
    Ok(())
}

pub fn add_instance_attributes(
    recordings: &mut [ExecutionRecordingSnapshot],
    service_env: &str,
    service_name: &str,
    instance_id: Uuid,
) {
    for single_rec in recordings {
        single_rec.attributes.insert(
            "service_env".to_string(),
            HashSet::from([service_env.to_string()]),
        );
        single_rec.attributes.insert(
            "service_name".to_string(),
            HashSet::from([service_name.to_string()]),
        );
        single_rec.attributes.insert(
            "instance_id".to_string(),
            HashSet::from([instance_id.to_string()]),
        );
    }
}
