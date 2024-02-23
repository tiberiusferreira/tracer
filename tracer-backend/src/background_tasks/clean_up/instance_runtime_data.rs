use crate::api::state::{ServiceRuntimeData, Shared};
use api_structs::ServiceId;
use std::collections::HashMap;
use tracing::{info, instrument};

#[instrument(skip_all)]
pub fn clean_up_dead_instances_and_services(
    services_runtime_stats: Shared<HashMap<ServiceId, ServiceRuntimeData>>,
) {
    info!("clean up service");
    services_runtime_stats
        .write()
        .retain(|service_id, runtime_data| {
            runtime_data
                .instances
                .retain(|instance_id, instance_state| {
                    info!(
                        "Checking instance {} of service {:?}",
                        instance_id, service_id
                    );
                    if instance_state.is_dead() {
                        info!("Instance is dead");
                        false
                    } else {
                        info!("Instance is alive");
                        true
                    }
                });
            if runtime_data.instances.is_empty() {
                false
            } else {
                true
            }
        });
}
