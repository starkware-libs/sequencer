pub mod config;
pub mod runner;
#[cfg(test)]
mod test;

use std::cmp::min;
use std::sync::Arc;

use apollo_class_manager_types::SharedClassManagerClient;
use apollo_infra::component_definitions::{ComponentRequestHandler, ComponentStarter};
use apollo_infra::component_server::{ConcurrentLocalComponentServer, RemoteComponentServer};
use apollo_starknet_client::reader::{StarknetFeederGatewayClient, StarknetReader};
use apollo_state_sync_types::communication::{StateSyncRequest, StateSyncResponse};
use apollo_state_sync_types::errors::StateSyncError;
use apollo_state_sync_types::state_sync_types::{StateSyncResult, SyncBlock};
use apollo_storage::body::BodyStorageReader;
use apollo_storage::db::TransactionKind;
use apollo_storage::header::HeaderStorageReader;
use apollo_storage::state::{StateReader, StateStorageReader};
use apollo_storage::{StorageReader, StorageTxn};
use async_trait::async_trait;
use futures::channel::mpsc::{channel, Sender};
use futures::SinkExt;
use lazy_static::lazy_static;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::{ClassHash, ContractAddress, Nonce, BLOCK_HASH_TABLE_ADDRESS};
use starknet_api::felt;
use starknet_api::state::{StateNumber, StorageKey};
use starknet_api::transaction::{Transaction, TransactionHash};
use starknet_types_core::felt::Felt;

use crate::config::StateSyncConfig;
use crate::runner::StateSyncRunner;

const BUFFER_SIZE: usize = 100000;

pub fn create_state_sync_and_runner(
    config: StateSyncConfig,
    class_manager_client: SharedClassManagerClient,
) -> (StateSync, StateSyncRunner) {
    let (new_block_sender, new_block_receiver) = channel(BUFFER_SIZE);
    let (state_sync_runner, storage_reader) =
        StateSyncRunner::new(config.clone(), new_block_receiver, class_manager_client);
    (StateSync::new(storage_reader, new_block_sender, config), state_sync_runner)
}

#[derive(Clone)]
pub struct StateSync {
    storage_reader: StorageReader,
    new_block_sender: Sender<SyncBlock>,
    starknet_client: Option<Arc<dyn StarknetReader + Send + Sync>>,
}

impl StateSync {
    pub fn new(
        storage_reader: StorageReader,
        new_block_sender: Sender<SyncBlock>,
        config: StateSyncConfig,
    ) -> Self {
        let starknet_client = config.central_sync_client_config.map(|config| {
            let config = config.central_source_config;
            let starknet_client: Arc<dyn StarknetReader + Send + Sync> = Arc::new(
                StarknetFeederGatewayClient::new(
                    config.starknet_url.as_ref(),
                    config.http_headers,
                    // TODO(shahak): fill with a proper version, or allow not specifying the
                    // node version.
                    "",
                    config.retry_config,
                )
                .expect("Failed creating feeder gateway client"),
            );
            starknet_client
        });
        Self { storage_reader, new_block_sender, starknet_client }
    }
}

// TODO(shahak): Have StateSyncRunner call StateSync instead of the opposite once we stop supporting
// papyrus executable and can move the storage into StateSync.
#[async_trait]
impl ComponentRequestHandler<StateSyncRequest, StateSyncResponse> for StateSync {
    async fn handle_request(&mut self, request: StateSyncRequest) -> StateSyncResponse {
        match request {
            StateSyncRequest::GetBlock(block_number) => {
                StateSyncResponse::GetBlock(self.get_block(block_number).await.map(Box::new))
            }
            StateSyncRequest::GetBlockHash(block_number) => {
                StateSyncResponse::GetBlockHash(self.get_block_hash(block_number).await)
            }
            StateSyncRequest::AddNewBlock(sync_block) => StateSyncResponse::AddNewBlock(
                self.new_block_sender.send(*sync_block).await.map_err(StateSyncError::from),
            ),
            StateSyncRequest::GetStorageAt(block_number, contract_address, storage_key) => {
                StateSyncResponse::GetStorageAt(
                    self.get_storage_at(block_number, contract_address, storage_key).await,
                )
            }
            StateSyncRequest::GetNonceAt(block_number, contract_address) => {
                StateSyncResponse::GetNonceAt(
                    self.get_nonce_at(block_number, contract_address).await,
                )
            }
            StateSyncRequest::GetClassHashAt(block_number, contract_address) => {
                StateSyncResponse::GetClassHashAt(
                    self.get_class_hash_at(block_number, contract_address).await,
                )
            }
            StateSyncRequest::GetLatestBlockNumber() => {
                StateSyncResponse::GetLatestBlockNumber(self.get_latest_block_number().await)
            }
            // TODO(shahak): Add tests for is_class_declared_at.
            StateSyncRequest::IsClassDeclaredAt(block_number, class_hash) => {
                StateSyncResponse::IsClassDeclaredAt(
                    self.is_class_declared_at(block_number, class_hash).await,
                )
            }
        }
    }
}

impl StateSync {
    async fn get_block(&self, block_number: BlockNumber) -> StateSyncResult<SyncBlock> {
        let storage_reader = self.storage_reader.clone();
        tokio::task::spawn_blocking(move || {
            let txn = storage_reader.begin_ro_txn()?;

            let block_not_found_err = Err(StateSyncError::BlockNotFound(block_number));
            let Some(block_header) = txn.get_block_header(block_number)? else {
                return block_not_found_err;
            };
            let Some(block_transactions_with_hash) =
                txn.get_block_transactions_with_hash(block_number)?
            else {
                return block_not_found_err;
            };
            let Some(thin_state_diff) = txn.get_state_diff(block_number)? else {
                return block_not_found_err;
            };

            let mut l1_transaction_hashes: Vec<TransactionHash> = vec![];
            let mut account_transaction_hashes: Vec<TransactionHash> = vec![];
            for (tx, tx_hash) in block_transactions_with_hash {
                match tx {
                    Transaction::L1Handler(_) => l1_transaction_hashes.push(tx_hash),
                    _ => account_transaction_hashes.push(tx_hash),
                }
            }

            Ok(SyncBlock {
                state_diff: thin_state_diff,
                block_header_without_hash: block_header.block_header_without_hash,
                account_transaction_hashes,
                l1_transaction_hashes,
            })
        })
        .await?
    }

    async fn get_block_hash(&self, block_number: BlockNumber) -> StateSyncResult<BlockHash> {
        // Getting the next block because the Sync block only contains parent hash.
        match (self.get_block(block_number.unchecked_next()).await, self.starknet_client.as_ref()) {
            (Ok(block), _) => Ok(block.block_header_without_hash.parent_hash),
            (Err(StateSyncError::BlockNotFound(_)), Some(starknet_client)) => {
                // As a fallback, try to get the block hash through the feeder directly. This
                // method is faster than get_block which the sync runner uses.
                // TODO(shahak): Test this flow.
                starknet_client
                    .block_hash(block_number)
                    .await?
                    .ok_or(StateSyncError::BlockNotFound(block_number))
            }
            (Err(err), _) => Err(err),
        }
    }

    async fn get_storage_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
        storage_key: StorageKey,
    ) -> StateSyncResult<Felt> {
        let storage_reader = self.storage_reader.clone();
        tokio::task::spawn_blocking(move || {
            let txn = storage_reader.begin_ro_txn()?;
            verify_synced_up_to(&txn, block_number)?;

            let state_number = StateNumber::unchecked_right_after_block(block_number);
            let state_reader = txn.get_state_reader()?;

            verify_contract_deployed(&state_reader, state_number, contract_address)?;

            let res = state_reader.get_storage_at(state_number, &contract_address, &storage_key)?;

            Ok(res)
        })
        .await?
    }

    async fn get_nonce_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
    ) -> StateSyncResult<Nonce> {
        let storage_reader = self.storage_reader.clone();
        tokio::task::spawn_blocking(move || {
            let txn = storage_reader.begin_ro_txn()?;
            verify_synced_up_to(&txn, block_number)?;

            let state_number = StateNumber::unchecked_right_after_block(block_number);
            let state_reader = txn.get_state_reader()?;

            verify_contract_deployed(&state_reader, state_number, contract_address)?;

            let res = state_reader
                .get_nonce_at(state_number, &contract_address)?
                .ok_or(StateSyncError::ContractNotFound(contract_address))?;

            Ok(res)
        })
        .await?
    }

    async fn get_class_hash_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
    ) -> StateSyncResult<ClassHash> {
        let storage_reader = self.storage_reader.clone();
        tokio::task::spawn_blocking(move || {
            let txn = storage_reader.begin_ro_txn()?;
            verify_synced_up_to(&txn, block_number)?;

            let state_number = StateNumber::unchecked_right_after_block(block_number);
            let state_reader = txn.get_state_reader()?;
            let class_hash = state_reader
                .get_class_hash_at(state_number, &contract_address)?
                .ok_or(StateSyncError::ContractNotFound(contract_address))?;
            Ok(class_hash)
        })
        .await?
    }

    async fn get_latest_block_number(&self) -> StateSyncResult<Option<BlockNumber>> {
        let storage_reader = self.storage_reader.clone();
        tokio::task::spawn_blocking(move || {
            let txn = storage_reader.begin_ro_txn()?;
            let latest_block_number = latest_synced_block(&txn)?;
            Ok(latest_block_number)
        })
        .await?
    }

    async fn is_class_declared_at(
        &self,
        block_number: BlockNumber,
        class_hash: ClassHash,
    ) -> StateSyncResult<bool> {
        // TODO(shahak): Remove this once we've resynced all production nodes.
        if OLD_DEPLOY_CLASS_HASH_WHITELIST.contains(&class_hash) {
            return Ok(true);
        }
        let storage_reader = self.storage_reader.clone();
        tokio::task::spawn_blocking(move || {
            let class_definition_block_number_opt = storage_reader
                .begin_ro_txn()?
                .get_state_reader()?
                .get_class_definition_block_number(&class_hash)?;
            if let Some(class_definition_block_number) = class_definition_block_number_opt {
                return Ok(class_definition_block_number <= block_number);
            }

            // TODO(noamsp): Add unit testing for cairo0
            let deprecated_class_definition_block_number_opt = storage_reader
                .begin_ro_txn()?
                .get_state_reader()?
                .get_deprecated_class_definition_block_number(&class_hash)?;

            Ok(deprecated_class_definition_block_number_opt.is_some_and(
                |deprecated_class_definition_block_number| {
                    deprecated_class_definition_block_number <= block_number
                },
            ))
        })
        .await?
    }
}

fn verify_synced_up_to<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
    block_number: BlockNumber,
) -> Result<(), StateSyncError> {
    if let Some(latest_block_number) = latest_synced_block(txn)? {
        if latest_block_number >= block_number {
            return Ok(());
        }
    }

    Err(StateSyncError::BlockNotFound(block_number))
}

fn latest_synced_block<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
) -> StateSyncResult<Option<BlockNumber>> {
    let latest_state_block_number = txn.get_state_marker()?.prev();
    if latest_state_block_number.is_none() {
        return Ok(None);
    }

    let latest_transaction_block_number = txn.get_body_marker()?.prev();
    if latest_transaction_block_number.is_none() {
        return Ok(None);
    }

    Ok(min(latest_state_block_number, latest_transaction_block_number))
}

fn verify_contract_deployed<Mode: TransactionKind>(
    state_reader: &StateReader<'_, Mode>,
    state_number: StateNumber,
    contract_address: ContractAddress,
) -> Result<(), StateSyncError> {
    // Contract address 0x1 is a special address, it stores the block
    // hashes. Contracts are not deployed to this address.
    if contract_address != BLOCK_HASH_TABLE_ADDRESS {
        // check if the contract is deployed
        state_reader
            .get_class_hash_at(state_number, &contract_address)?
            .ok_or(StateSyncError::ContractNotFound(contract_address))?;
    };

    Ok(())
}

pub type LocalStateSyncServer =
    ConcurrentLocalComponentServer<StateSync, StateSyncRequest, StateSyncResponse>;
pub type RemoteStateSyncServer = RemoteComponentServer<StateSyncRequest, StateSyncResponse>;

impl ComponentStarter for StateSync {}

lazy_static! {
    static ref OLD_DEPLOY_CLASS_HASH_WHITELIST: [ClassHash; 135] = [
        ClassHash(felt!("00a4e57ac025a86e283b7ef0132a6543305600788b783ba6fb48426703a5abbf")),
        ClassHash(felt!("03f43dfee1c6e5c33065a714535e06a8ffd7e76a12bb74ed9093946be6c2d798")),
        ClassHash(felt!("0726edb35cc732c1b3661fd837592033bd85ae8dde31533c35711fb0422d8993")),
        ClassHash(felt!("07ab58843a19e516c83057cd49beaba917aa9477cab703b3c6ae1ab9d822c4b1")),
        ClassHash(felt!("07f1c7e439328634d159aeb68f1b40b74f7a05da3832515561e77aadaf57a591")),
        ClassHash(felt!("0157d87bdad1328cbf429826a83545f6ffb6505138983885a75997ee2c49e66b")),
        ClassHash(felt!("075da0ac40ebc084e87ba5f22f1c2743a9e7e85d88dc795d0379cd00d04a7072")),
        ClassHash(felt!("07b5e991587f0c59db1c4c4ff9b26fa8ec49198ca6d8a82823cc2c6177d918fa")),
        ClassHash(felt!("0665d8f3ff5d12d8e1f4bcfea7b54f10eb5fe294cac5a428d6549eba08e00451")),
        ClassHash(felt!("07595b4f7d50010ceb00230d8b5656e3c3dd201b6df35d805d3f2988c69a1432")),
        ClassHash(felt!("03ec0f1fa8614a821596bae6ecbacca66b52b5c1c92fe1a950656b9a3beaf012")),
        ClassHash(felt!("0631217418b8965601291874c906989517fa934babbce7a7ea18a7668c7c6304")),
        ClassHash(felt!("05d9a7bdec373b49efef91c6da3d595a96e1a5e8302bc62c7c8ac7df730e751d")),
        ClassHash(felt!("020c279bb3d77dce970cf5dc98341251bbc20eb021d029a8fd28020a4bd9c3c1")),
        ClassHash(felt!("0733734fa0dab1158bccdfe0df7b0becf3827f908971fac8d39cc73d99ad8645")),
        ClassHash(felt!("0784f488f47e20acfc738456581c2c7d34e7a0b74b040a8ab347f24f10c00b6e")),
        ClassHash(felt!("04d07e40e93398ed3c76981e72dd1fd22557a78ce36c0515f679e27f0bb5bc5f")),
        ClassHash(felt!("069236f4ab1d052ca50b7574436e6a6b760503d7973bb7133255a2676c1bdb69")),
        ClassHash(felt!("008b12f11b1ea5e43cdf8870be6a0579e920873a83c4b8931a27abc4c032ee64")),
        ClassHash(felt!("039547c16c9ca7ab54882da944161eb7b13805e5dbb01dab45d814baada6f282")),
        ClassHash(felt!("0626424ef94979b877311ac1be3099b2021977d281e5c8c66acc92b40ba1e1df")),
        ClassHash(felt!("057a4fbfcfcde85f3d99ca1053266cb29c513856fd2a758df6774def2101e677")),
        ClassHash(felt!("023d98a23d5f542269486edc2b9c0394cc510a9862c3f3ea82fa0798269ee0bc")),
        ClassHash(felt!("008d8726481b7b8a4a8f95aa2ce6a18932c33148600c75b1b4fe40c5987c6c9a")),
        ClassHash(felt!("04c53698c9a42341e4123632e87b752d6ae470ddedeb8b0063eaa2deea387eeb")),
        ClassHash(felt!("066a3ec9166b2b39de4bb5f46dee36998c29d2ae3bc61bd1187fb11fa676555b")),
        ClassHash(felt!("046f844ea1a3b3668f81d38b5c1bd55e816e0373802aefe732138628f0133486")),
        ClassHash(felt!("04e709a7d569e5b041f46e0d4d654cbf96d8b4749088705892d4e6969dff3e35")),
        ClassHash(felt!("029d2cc5f6beac7fef596a256c8bd3a0eb0f83248886599ec80ee72133d747eb")),
        ClassHash(felt!("017eddd685efc7966e9ceb26d5ccf58180f802c72f06f1a1bff1c4d74a59c872")),
        ClassHash(felt!("03131fa018d520a037686ce3efddeab8f28895662f019ca3ca18a626650f7d1e")),
        ClassHash(felt!("06dadf634444dfa550750191a1b8ba77970d0f11bea3df469c968bb59ba73158")),
        ClassHash(felt!("002f09c8cc7698d6a220b929b525efd5b3622a7658e9a9e4c8a52d5ed995d6fe")),
        ClassHash(felt!("07e35b811e3d4e2678d037b632a0c8a09a46d82185187762c0d429363e3ef9cf")),
        ClassHash(felt!("0042477e2f668af48c85770ffc9ececcd39d5aaf077da491a5bdc6204f71bdb6")),
        ClassHash(felt!("02c3348ad109f7f3967df6494b3c48741d61675d9a7915b265aa7101a631dc33")),
        ClassHash(felt!("04a153d1ef11d0bdff01e51ae866e7a8c66f2e968cfeabcb30793220b5910e2f")),
        ClassHash(felt!("07d32ee16204aef3a9e4ed9b8b681fda37ab299e29fe1b21413ce3768d4e4a76")),
        ClassHash(felt!("060924bf85cdd22c7e24cf8b6ac45a9a6b753671dfd0da07aae5d6b3c65a05e2")),
        ClassHash(felt!("070aa6f103ec0e73e7acb691f0f05208d6b1cd67c309e22f4e2605db109600f1")),
        ClassHash(felt!("027f27c5dcec1611d303e65ddb12acc92def6202203c5b3f8762855306d01065")),
        ClassHash(felt!("010455c752b86932ce552f2b0fe81a880746649b9aee7e0d842bf3f52378f9f8")),
        ClassHash(felt!("008dbcb158b134f80d7cdfc2afe7c1fca21de99865d41b9b17bc3303432e8c51")),
        ClassHash(felt!("01cb96b938da26c060d5fd807eef8b580c49490926393a5eeb408a89f84b9b46")),
        ClassHash(felt!("04e6d8a7f0b6178e2e8f0789e34925fe425b761b0bb608916b0d86531c65c16d")),
        ClassHash(felt!("0167470236540c4537a005a1d1cabc08e7ebb10a6f75c9a1d1171a131e41a95b")),
        ClassHash(felt!("033143c31c75a787ebdb16a3d9400e0d1a71fbeac2a067793bf93c289d56b03b")),
        ClassHash(felt!("0399f629212fd7f2f06723f84340fddd48720c6a1bc19c159313138520a097d5")),
        ClassHash(felt!("05605533726157c4b3aa2213365539cad99273b993bc35e91043a8cae42cd402")),
        ClassHash(felt!("026033711f79998aadcaee45eab3d8b7d04d83f2efb3334cf38db1fbd947339e")),
        ClassHash(felt!("07c44f58c7a0f03c74f96ba5cfd4e3b1036411d5cca1e65695ef1ae11751414d")),
        ClassHash(felt!("07c8296b1c53c8000b10f54c5b2aa6a36dd4a69b1074dafdf7d0215403ae3e27")),
        ClassHash(felt!("01ba5d32901bb498ab120297923786da8f3425d2ad518c3ceedf9fa2723a0eaa")),
        ClassHash(felt!("02dbfa1499840ded1107c63d3ddf4612e0765ef69d511ddaaebc6bf91bb1388d")),
        ClassHash(felt!("034de8f05f757772514aafe058245731ba93796cbcf1274fca6ba1d04eeac950")),
        ClassHash(felt!("00d0e183745e9dae3e4e78a8ffedcce0903fc4900beace4e0abf192d4c202da3")),
        ClassHash(felt!("055dd147f5ac39904b109b7407396529ab6defb8feef46a147e742accd6f8795")),
        ClassHash(felt!("011eb881c6330b42c1d6d0d37581c134ab51235850bebbc604ebf96ef8dea5d7")),
        ClassHash(felt!("0037150ba6f2ccb3a19a45ebe2de28e85b21dbdcaf77436f4e0cdf686a109989")),
        ClassHash(felt!("048498ebae1afc22157322db4bb7814b668c7ee20237cc8be64d934649679da1")),
        ClassHash(felt!("07543f8eb21f10b1827a495084697a519274ac9c1a1fbf931bac40133a6b9c15")),
        ClassHash(felt!("048a7114e47629575c260b184686018c61c34055fefc53f6c100dedf3bc10eba")),
        ClassHash(felt!("00edf4a4a2928afc19b038ab799899e0e179c7abdd86f5f1c0c5dd1b307a6b96")),
        ClassHash(felt!("039a43c9b60a8f5f8de4c06a163b3f24bf9e59f1a889c323ba5b5578b1d90d95")),
        ClassHash(felt!("0064aaf619d5b32b5496cb17d64ad38f390598442d6ac9a2d72c6280b863efba")),
        ClassHash(felt!("0639a9186c8d9daa7c44186e7cf987cc458a46965e2658d27020554f9671677b")),
        ClassHash(felt!("071c3c99f5cf76fc19945d4b8b7d34c7c5528f22730d56192b50c6bbfd338a64")),
        ClassHash(felt!("078dac186418f94cb1eb2a32ed909a6e81028ce8b40b9ec0fe7f6fe4e11addd8")),
        ClassHash(felt!("07d72193cd1a0a4ae7ad8b478bc7c431ce0af2803d20726253eb3e18f13f39f6")),
        ClassHash(felt!("03115d29825b1a3563ada334fd4b55b36bef7f9ed01e76abe9fd0d4be616a1a7")),
        ClassHash(felt!("0621b53649ddc3170f4c7db984252e9de2fd54c8080686aa9aaf5803194a9179")),
        ClassHash(felt!("0750cd490a7cd1572411169eaa8be292325990d33c5d4733655fe6b926985062")),
        ClassHash(felt!("071b7f73b5e2b4f81f7cf01d4d1569ccba2921b3fa3170cf11cff3720dfe918e")),
        ClassHash(felt!("02895b8c4c21ed47527fa779bab13857247580553dc9f66f0a6f2146110a5a29")),
        ClassHash(felt!("05915aa48a8b558dacbab894ff2e0062c6957b0b01ae89b2c609f905b7f4e19d")),
        ClassHash(felt!("07122c9477ac445f6bf217176b5bc0e2d059b86f32f914a7500d9fd8f24e38ef")),
        ClassHash(felt!("05e5bad04b75726fb33f4f146193ab9640a80d9fbc8f2a569192efb086b72307")),
        ClassHash(felt!("031008229e9725331feb1ceff338c0e355139de300935285c2e3b76025c4abd0")),
        ClassHash(felt!("026fe8ea36ec7703569cfe4693b05102940bf122647c4dbf0abc0bb919ce27bd")),
        ClassHash(felt!("022a7efd343b272e499c4ae2c4250f9d345ec50a1daae97cfced921c99775ff9")),
        ClassHash(felt!("014eb6c9903997faff3ad620aed584f2d0dccba5c5a016863c14228b319e09ec")),
        ClassHash(felt!("00d245a36e35446ccff2ac0c6bf8d1054c970b3f5f8567113e9985724009be5a")),
        ClassHash(felt!("03bcf9bad9f636f13527bde8273473314eac6f428956c03fadae1c247d03633f")),
        ClassHash(felt!("00655101fb9a3c803a50de65fe9e347a71300e0f9db058da5b8ddebca2681a93")),
        ClassHash(felt!("0142f4510d6a58a01091bc5ac9273535868ef6e5c8ac579ad6970c479e0553f1")),
        ClassHash(felt!("06b4ee05bc5d19e932c20e5ffc2c184f1d844627a3bfebba1f0d03a2f811145f")),
        ClassHash(felt!("06a1776964b9f991c710bfe910b8b37578b32b26a7dffd1669a1a59ac94bf82f")),
        ClassHash(felt!("02073b7903c36c9c126c580da737b5b80bd5d77b856ae2b3906a412bbbf47480")),
        ClassHash(felt!("044b511cbb25497a1534b4d821a347563a336a3955024a263786b3f19d639c11")),
        ClassHash(felt!("03b2a3daa8c6f39fa07ad9847595f1841325ae7a993d6f1919f364235fd62a4b")),
        ClassHash(felt!("049c599d777d52365e0e300a24067a0c420776929d398e760660170cb2212248")),
        ClassHash(felt!("02cdf5ac65a41b135969dcefa9d52799a48994d4d3aee24732b78580a9fa7c63")),
        ClassHash(felt!("06e26cd8efb04ca3ebb27d7b7ce32b2c6668316f26b8569f5c1cf497f27b259b")),
        ClassHash(felt!("05829a410055a7da53295c05b7ce39f1b99c202d49f6194ca000d93d35adf491")),
        ClassHash(felt!("07b553907c45d4be5b72a7816068518f1a1e326cef33fb1d244852965da6e8eb")),
        ClassHash(felt!("0031da92cf5f54bcb81b447e219e2b791b23f3052d12b6c9abd04ff2e5626576")),
        ClassHash(felt!("041efe53a722b4325d7f86b97c24b41eb6d16b98ae8a3f3374a8422033666a3e")),
        ClassHash(felt!("01a315bf4e9fb59fe57c20b3d959ede354c12786e315fc4c656a7112659aa31f")),
        ClassHash(felt!("07e24e482a41ba9c85e74fa62755f8dc1e378ba451cb9da2e89975d13dfa3117")),
        ClassHash(felt!("04d1f4cf4ef520c768a326d34f919227e1f075effda532f57cbaec6a1228db88")),
        ClassHash(felt!("00cc97194621de7ad998d64cda4554ef35c4e8931940997b2629e743eaccb4a7")),
        ClassHash(felt!("00631071be7e7f42a5445af1a4bd30f97b90495f9cb4a8a82d5a94eb08a25a1f")),
        ClassHash(felt!("066cc273d328e53eb56b3788684d22062bbe1a915c04594a476b33225d4ab1f4")),
        ClassHash(felt!("074a7ed7f1236225600f355efe70812129658c82c295ff0f8307b3fad4bf09a9")),
        ClassHash(felt!("03e44a8ce106126fbe5e777db2ca6faa7f53665e0c3d67b5794d5d95874e7472")),
        ClassHash(felt!("078a6a24cd6406b3f170fd99db0d2157618203180d79c5c6807bcf633972180d")),
        ClassHash(felt!("00c6529e402abdc91cc51f61ae71df2337bc6c535fd96eb79235b1a32bbc6fdc")),
        ClassHash(felt!("06921d057693ea95d25883c8e0ba59d047497c23083384d95cd5937c01ae22f6")),
        ClassHash(felt!("04225e15fc63c5b127e47238c9be8a403c1c75488eab215210126bcbdc48aec5")),
        ClassHash(felt!("074ddc960b4568092956dade86c2919a6f5c06524beefcb5a0a7b25ee3db250c")),
        ClassHash(felt!("02f885b664728291d97d8e4b27df3af8c58b69514fd825782153db9c32009763")),
        ClassHash(felt!("07797ede55ea525e24078b973467b40850311f9de31913fa13fc5ef92305d849")),
        ClassHash(felt!("068bb00f783b88aeb61551f5383e6b7f1621463cb570e8f63df89e3681045134")),
        ClassHash(felt!("02f07cff96aff7147d4434d5cba381b75fe3d77d6b1e7f9fdd94845761afd452")),
        ClassHash(felt!("0550a4b8f65f1fc1abd9f93527460c252e5a17fb2af1b5f46c981c7a646b707e")),
        ClassHash(felt!("057473cd614625f7bc6998840cd2c62fe4fff39834322ac74c16eb6aa2b8f2d3")),
        ClassHash(felt!("03f7a4ce5403d3a7417d9115a0982bf4bf2bc86bbfd881506a2fb466e41a8575")),
        ClassHash(felt!("05d26d8883e71933c33db502cc874c6366d536970d1a399ee3a5624c94022cb2")),
        ClassHash(felt!("07197021c108b0cc57ae354f5ad02222c4b3d7344664e6dd602a0e2298595434")),
        ClassHash(felt!("0228a118243209dfe39069d25be535590435227e4df558cbcc54e70913b515d3")),
        ClassHash(felt!("033f02ceeb057e36174b12a640376e5ade0c16a31a20ea129e7601160f85d383")),
        ClassHash(felt!("04e840d1641e38c1f32d57ff062804c8666823f541a508b341651c8fa9d942dd")),
        ClassHash(felt!("025529e95e8f10bef74fb4adb395652ccba2707d98e7eaa114a234f12385b447")),
        ClassHash(felt!("02693dd12b58ce4da887ee9530a79c71bdeb739cf888ae9528f58d513abbd336")),
        ClassHash(felt!("0710c5eedefc2c2e2dff5d6c432fda3e1e4af93579a3dbb7ee55cf260fadd543")),
        ClassHash(felt!("06d135aff146d54d4dea40cdd0588845788468d1cdc0bddf5138d98567d3a7c3")),
        ClassHash(felt!("01d492f0765a73712d0cd6654433945c8ac5b3409862cff41e7260315517747b")),
        ClassHash(felt!("055dd2a2cf55321ff7130d68fc941a0d887b19e46eff6eae4a13b1f02bc231f6")),
        ClassHash(felt!("0108a32ec851d37c8f15387dadc87dc80c302c5278b24211ea5b227a4bfdc752")),
        ClassHash(felt!("071891339979b295508cd61a8477959484b955c08a617cec54ade3ccedf63762")),
        ClassHash(felt!("0248339769e0b06b7d259bcaec19756d883c8610770c4f77c750a04f2401ea3b")),
        ClassHash(felt!("04c2879de40a4af38083026ebf92c40dc674d7148b2369f2adb1ca155b995c30")),
        ClassHash(felt!("05cc0c9569aa4e7845780a3359c88f847003bbf8bede6a55bad96da7b9fb4d50")),
        ClassHash(felt!("04572af1cd59b8b91055ebb78df8f1d11c59f5270018b291366ba4585d4cdff0")),
        ClassHash(felt!("05109f0f2e1db1a9977f4e59361041fd4b63988aa8bee898becb47f87307cfd4")),
    ];
}
