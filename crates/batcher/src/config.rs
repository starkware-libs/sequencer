use std::collections::BTreeMap;

use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::proposal_manager::ProposalManagerConfig;

/// The batcher related configuration.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct BatcherConfig {
    pub proposal_manager: ProposalManagerConfig,
    pub outstream_content_buffer_size: usize,
}

impl SerializeConfig for BatcherConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        // TODO(yair): create nicer function to append sub configs.
        let mut dump = append_sub_config_name(self.proposal_manager.dump(), "proposal_manager");
        dump.append(&mut BTreeMap::from([ser_param(
            "outstream_content_buffer_size",
            &self.outstream_content_buffer_size,
            "Maximum items to add to the outstream buffer before blocking further filling of the \
             stream",
            ParamPrivacyInput::Public,
        )]));
        dump
    }
}
