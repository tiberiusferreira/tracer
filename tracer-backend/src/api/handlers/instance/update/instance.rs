use crate::api::handlers::instance::update::{InstanceServiceInformation, ProcessUpdateError};
use api_structs::instance::update::ExecutionRecordingSnapshot;
use gel_io_recorder::{Parameter, Transaction};
use std::collections::{HashSet};
use indexmap::IndexMap;
use uuid::Uuid;

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
    let params = IndexMap::from([("latest_profile_base64", Parameter::from(profile))]);
    let updated = tx.update("ServiceInstance", instance_id, params).await?;
    assert!(updated);
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
