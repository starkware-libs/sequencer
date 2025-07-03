use std::collections::BTreeMap;

use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use blockifier::blockifier::config::ContractClassManagerConfig;
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

use crate::block_builder::BlockBuilderConfig;
use crate::pre_confirmed_block_writer::PreconfirmedBlockWriterConfig;
use crate::pre_confirmed_cende_client::PreconfirmedCendeConfig;

/// The batcher related configuration.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
#[validate(schema(function = "validate_batcher_config"))]
pub struct BatcherConfig {
    pub storage: apollo_storage::StorageConfig,
    pub outstream_content_buffer_size: usize,
    pub input_stream_content_buffer_size: usize,
    pub block_builder_config: BlockBuilderConfig,
    pub pre_confirmed_block_writer_config: PreconfirmedBlockWriterConfig,
    pub contract_class_manager_config: ContractClassManagerConfig,
    pub max_l1_handler_txs_per_block_proposal: usize,
    pub pre_confirmed_cende_config: PreconfirmedCendeConfig,
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
        dump.append(&mut prepend_sub_config_name(self.storage.dump(), "storage"));
        dump.append(&mut prepend_sub_config_name(
            self.block_builder_config.dump(),
            "block_builder_config",
        ));
        dump.append(&mut prepend_sub_config_name(
            self.pre_confirmed_block_writer_config.dump(),
            "pre_confirmed_block_writer_config",
        ));
        dump.append(&mut prepend_sub_config_name(
            self.contract_class_manager_config.dump(),
            "contract_class_manager_config",
        ));
        dump.append(&mut prepend_sub_config_name(
            self.pre_confirmed_cende_config.dump(),
            "pre_confirmed_cende_config",
        ));
        dump
    }
}

impl Default for BatcherConfig {
    fn default() -> Self {
        Self {
            storage: apollo_storage::StorageConfig {
                db_config: apollo_storage::db::DbConfig {
                    path_prefix: "/data/batcher".into(),
                    enforce_file_exists: false,
                    ..Default::default()
                },
                scope: apollo_storage::StorageScope::StateOnly,
                ..Default::default()
            },
            // TODO(AlonH): set a more reasonable default value.
            outstream_content_buffer_size: 100,
            input_stream_content_buffer_size: 400,
            block_builder_config: BlockBuilderConfig::default(),
            pre_confirmed_block_writer_config: PreconfirmedBlockWriterConfig::default(),
            contract_class_manager_config: ContractClassManagerConfig::default(),
            max_l1_handler_txs_per_block_proposal: 3,
            pre_confirmed_cende_config: PreconfirmedCendeConfig::default(),
        }
    }
}

fn validate_batcher_config(batcher_config: &BatcherConfig) -> Result<(), ValidationError> {
    if batcher_config.input_stream_content_buffer_size
        < batcher_config.block_builder_config.n_concurrent_txs
    {
        return Err(ValidationError::new(
            "input_stream_content_buffer_size must be at least n_concurrent_txs",
        ));
    }
    Ok(())
}
