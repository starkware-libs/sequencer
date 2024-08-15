pub mod receiver;
pub mod sender;

use papyrus_network::NetworkConfig;

use crate::receiver::MempoolP2PReceiver;
use crate::sender::MempoolP2PSender;

pub fn create_p2p_sender_and_receiver(
    _network_config: NetworkConfig,
) -> (MempoolP2PSender, MempoolP2PReceiver) {
    unimplemented!()
}
