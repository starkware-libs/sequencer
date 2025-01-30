use std::collections::BTreeMap;

use blockifier::blockifier::config::ContractClassManagerConfig;
use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

use crate::block_builder::BlockBuilderConfig;

/// The batcher related configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
#[validate(schema(function = "validate_batcher_config"))]
pub struct BatcherConfig {
    pub storage: papyrus_storage::StorageConfig,
    pub outstream_content_buffer_size: usize,
    pub input_stream_content_buffer_size: usize,
    pub block_builder_config: BlockBuilderConfig,
    pub contract_class_manager_config: ContractClassManagerConfig,
    pub max_l1_handler_txs_per_block_proposal: usize,
}

impl SerializeConfig for BatcherConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        // TODO(yair): create nicer function to append sub configs.
        let mut dump = BTreeMap::from([
            ser_param(
                "outstream_content_buffer_size",
                &self.outstream_content_buffer_size,
                "The maximum number of items to include in a single get_proposal_content response.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "input_stream_content_buffer_size",
                &self.input_stream_content_buffer_size,
                "Sets the buffer size for the input transaction channel. Adding more transactions \
                 beyond this limit will block until space is available.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_l1_handler_txs_per_block_proposal",
                &self.max_l1_handler_txs_per_block_proposal,
                "The maximum number of L1 handler transactions to include in a block proposal.",
                ParamPrivacyInput::Public,
            ),
        ]);
        dump.append(&mut append_sub_config_name(self.storage.dump(), "storage"));
        dump.append(&mut append_sub_config_name(
            self.block_builder_config.dump(),
            "block_builder_config",
        ));
        dump.append(&mut append_sub_config_name(
            self.contract_class_manager_config.dump(),
            "contract_class_manager_config",
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
            // TODO: set a more reasonable default value.
            outstream_content_buffer_size: 100,
            input_stream_content_buffer_size: 400,
            block_builder_config: BlockBuilderConfig::default(),
            contract_class_manager_config: ContractClassManagerConfig::default(),
            max_l1_handler_txs_per_block_proposal: 3,
        }
    }
}

fn validate_batcher_config(batcher_config: &BatcherConfig) -> Result<(), ValidationError> {
    if batcher_config.input_stream_content_buffer_size
        < batcher_config.block_builder_config.tx_chunk_size
    {
        return Err(ValidationError::new(
            "input_stream_content_buffer_size must be at least tx_chunk_size",
        ));
    }
    Ok(())
}
