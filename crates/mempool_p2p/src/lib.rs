pub mod receiver;
pub mod sender;

use papyrus_network::NetworkConfig;

use crate::receiver::MempoolP2pReceiver;
use crate::sender::MempoolP2pSender;

pub fn create_p2p_sender_and_receiver(
    _network_config: NetworkConfig,
) -> (MempoolP2pSender, MempoolP2pReceiver) {
    unimplemented!()
}
