use starknet_mempool_infra::component_server::{create_empty_server, WrapperServer};

use crate::sequencer_monitoring_endpoint::SequencerMonitoringEndpoint;

pub type SequencerMonitoringServer = WrapperServer<SequencerMonitoringEndpoint>;

pub fn create_sequencer_monitoring_server(
    sequencer_monitoring_endpont: SequencerMonitoringEndpoint,
) -> SequencerMonitoringServer {
    create_empty_server(sequencer_monitoring_endpont)
}
