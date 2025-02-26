use starknet_sequencer_metrics::metric_definitions::CONSENSUS_HEIGHT;

pub(crate) fn register_metrics(){
    CONSENSUS_HEIGHT.register(); 
}