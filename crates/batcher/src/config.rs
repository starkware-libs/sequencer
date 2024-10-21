use std::collections::BTreeMap;

use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::block_builder::BlockBuilderConfig;
use crate::proposal_manager::ProposalManagerConfig;

/// The batcher related configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct BatcherConfig {
    pub storage: papyrus_storage::StorageConfig,
    pub proposal_manager: ProposalManagerConfig,
    pub outstream_content_buffer_size: usize,
    pub block_builder_config: BlockBuilderConfig,
    pub global_contract_cache_size: usize,
}

impl SerializeConfig for BatcherConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        // TODO(yair): create nicer function to append sub configs.
        let mut dump = append_sub_config_name(self.proposal_manager.dump(), "proposal_manager");
        dump.append(&mut BTreeMap::from([ser_param(
            "outstream_content_buffer_size",
            &self.outstream_content_buffer_size,
            "Maximum items to add to the outstream buffer before blocking further filling of the \
             stream.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut BTreeMap::from([ser_param(
            "global_contract_cache_size",
            &self.global_contract_cache_size,
            "Cache size for the global_class_hash_to_class. Initialized with this size on \
             creation.",
            ParamPrivacyInput::Public,
        )]));
        dump.append(&mut append_sub_config_name(self.storage.dump(), "storage"));
        dump.append(&mut append_sub_config_name(
            self.block_builder_config.dump(),
            "block_builder_config",
        ));
        dump
    }
}

impl Default for BatcherConfig {
    fn default() -> Self {
        Self {
            storage: papyrus_storage::StorageConfig {
                db_config: papyrus_storage::db::DbConfig {
                    path_prefix: ".".into(),
                    // By default we don't want to create the DB if it doesn't exist.
                    enforce_file_exists: true,
                    ..Default::default()
                },
                scope: papyrus_storage::StorageScope::StateOnly,
                ..Default::default()
            },
            proposal_manager: ProposalManagerConfig::default(),
            // TODO: set a more reasonable default value.
            outstream_content_buffer_size: 100,
            block_builder_config: BlockBuilderConfig::default(),
            global_contract_cache_size: 100,
        }
    }
}
