use crate::api::state::{ServiceRuntimeData, Shared};
use crate::{DEAD_INSTANCE_MAX_STATS_HISTORY_DATA_COUNT, DEAD_INSTANCE_RETENTION_TIME_SECONDS};
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
                    info!("Checking instance {} of service {:?}", instance_id, service_id);
                    if instance_state.is_dead(){
                        info!("Instance is dead");
                        let seconds_since_last_seen = instance_state.seconds_since_last_seen();
                        if (DEAD_INSTANCE_RETENTION_TIME_SECONDS as u64) < seconds_since_last_seen {
                            info!("Instance is dead and over retention time, removing it");
                            false
                        }else{
                            info!("Instance is dead and under retention time, keeping it, but trimming data points to {}", DEAD_INSTANCE_MAX_STATS_HISTORY_DATA_COUNT);
                            instance_state.trim_data_points_to(DEAD_INSTANCE_MAX_STATS_HISTORY_DATA_COUNT);
                            true
                        }
                    }else {
                        info!("Instance is alive");
                        true
                    }
                });
            if runtime_data.instances.is_empty(){
                false
            }else{
                true
            }
        });
}
