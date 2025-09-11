use async_trait::async_trait;
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use starknet_api::consensus_transaction::{ConsensusTransaction, InternalConsensusTransaction};
use starknet_api::contract_class::{ClassInfo, ContractClass, SierraVersion};
use starknet_api::core::{ChainId, ClassHash};
use starknet_api::executable_transaction::{
    AccountTransaction,
    Transaction as ExecutableTransaction,
    ValidateCompiledClassHashError,
};
use starknet_api::rpc_transaction::{
    InternalRpcDeclareTransactionV3,
    InternalRpcDeployAccountTransaction,
    InternalRpcTransaction,
    InternalRpcTransactionWithoutTxHash,
    RpcDeclareTransaction,
    RpcDeclareTransactionV3,
    RpcDeployAccountTransaction,
    RpcTransaction,
};
use starknet_api::state::SierraContractClass;
use starknet_api::transaction::fields::Fee;
use starknet_api::transaction::CalculateContractAddress;
use starknet_api::{executable_transaction, transaction, StarknetApiError};
use thiserror::Error;

use crate::{ClassHashes, ClassManagerClientError, SharedClassManagerClient};

#[derive(Error, Debug, Clone)]
pub enum TransactionConverterError {
    #[error(transparent)]
    ClassManagerClientError(#[from] ClassManagerClientError),
    #[error("Class of hash: {class_hash} not found")]
    ClassNotFound { class_hash: ClassHash },
    #[error(transparent)]
    StarknetApiError(#[from] StarknetApiError),
    #[error(transparent)]
    ValidateCompiledClassHashError(#[from] ValidateCompiledClassHashError),
}

pub type TransactionConverterResult<T> = Result<T, TransactionConverterError>;

#[cfg_attr(any(test, feature = "testing"), automock)]
#[async_trait]
pub trait TransactionConverterTrait: Send + Sync {
    async fn convert_internal_consensus_tx_to_consensus_tx(
        &self,
        tx: InternalConsensusTransaction,
    ) -> TransactionConverterResult<ConsensusTransaction>;

    async fn convert_consensus_tx_to_internal_consensus_tx(
        &self,
        tx: ConsensusTransaction,
    ) -> TransactionConverterResult<InternalConsensusTransaction>;

    async fn convert_internal_rpc_tx_to_rpc_tx(
        &self,
        tx: InternalRpcTransaction,
    ) -> TransactionConverterResult<RpcTransaction>;

    async fn convert_rpc_tx_to_internal_rpc_tx(
        &self,
        tx: RpcTransaction,
    ) -> TransactionConverterResult<InternalRpcTransaction>;

    async fn convert_internal_rpc_tx_to_executable_tx(
        &self,
        tx: InternalRpcTransaction,
    ) -> TransactionConverterResult<AccountTransaction>;

    async fn convert_internal_consensus_tx_to_executable_tx(
        &self,
        tx: InternalConsensusTransaction,
    ) -> TransactionConverterResult<ExecutableTransaction>;
}

#[derive(Clone)]
pub struct TransactionConverter {
    class_manager_client: SharedClassManagerClient,
    chain_id: ChainId,
}

impl TransactionConverter {
    pub fn new(class_manager_client: SharedClassManagerClient, chain_id: ChainId) -> Self {
        Self { class_manager_client, chain_id }
    }

    async fn get_sierra(
        &self,
        class_hash: ClassHash,
    ) -> TransactionConverterResult<SierraContractClass> {
        self.class_manager_client
            .get_sierra(class_hash)
            .await?
            .ok_or(TransactionConverterError::ClassNotFound { class_hash })
    }

    async fn get_executable(
        &self,
        class_hash: ClassHash,
    ) -> TransactionConverterResult<ContractClass> {
        self.class_manager_client
            .get_executable(class_hash)
            .await?
            .ok_or(TransactionConverterError::ClassNotFound { class_hash })
    }
}

#[async_trait]
impl TransactionConverterTrait for TransactionConverter {
    async fn convert_internal_consensus_tx_to_consensus_tx(
        &self,
        tx: InternalConsensusTransaction,
    ) -> TransactionConverterResult<ConsensusTransaction> {
        match tx {
            InternalConsensusTransaction::RpcTransaction(tx) => self
                .convert_internal_rpc_tx_to_rpc_tx(tx)
                .await
                .map(ConsensusTransaction::RpcTransaction),
            InternalConsensusTransaction::L1Handler(tx) => {
                Ok(ConsensusTransaction::L1Handler(tx.tx))
            }
        }
    }

    async fn convert_consensus_tx_to_internal_consensus_tx(
        &self,
        tx: ConsensusTransaction,
    ) -> TransactionConverterResult<InternalConsensusTransaction> {
        match tx {
            ConsensusTransaction::RpcTransaction(tx) => self
                .convert_rpc_tx_to_internal_rpc_tx(tx)
                .await
                .map(InternalConsensusTransaction::RpcTransaction),
            ConsensusTransaction::L1Handler(tx) => self
                .convert_consensus_l1_handler_to_internal_l1_handler(tx)
                .map(InternalConsensusTransaction::L1Handler),
        }
    }

    async fn convert_internal_rpc_tx_to_rpc_tx(
        &self,
        tx: InternalRpcTransaction,
    ) -> TransactionConverterResult<RpcTransaction> {
        match tx.tx {
            InternalRpcTransactionWithoutTxHash::Invoke(tx) => Ok(RpcTransaction::Invoke(tx)),
            InternalRpcTransactionWithoutTxHash::Declare(tx) => {
                Ok(RpcTransaction::Declare(RpcDeclareTransaction::V3(RpcDeclareTransactionV3 {
                    sender_address: tx.sender_address,
                    compiled_class_hash: tx.compiled_class_hash,
                    signature: tx.signature,
                    nonce: tx.nonce,
                    contract_class: self.get_sierra(tx.class_hash).await?,
                    resource_bounds: tx.resource_bounds,
                    tip: tx.tip,
                    paymaster_data: tx.paymaster_data,
                    account_deployment_data: tx.account_deployment_data,
                    nonce_data_availability_mode: tx.nonce_data_availability_mode,
                    fee_data_availability_mode: tx.fee_data_availability_mode,
                })))
            }
            InternalRpcTransactionWithoutTxHash::DeployAccount(
                InternalRpcDeployAccountTransaction { tx, .. },
            ) => Ok(RpcTransaction::DeployAccount(tx)),
        }
    }

    async fn convert_rpc_tx_to_internal_rpc_tx(
        &self,
        tx: RpcTransaction,
    ) -> TransactionConverterResult<InternalRpcTransaction> {
        let tx_without_hash = match tx {
            RpcTransaction::Invoke(tx) => InternalRpcTransactionWithoutTxHash::Invoke(tx),
            RpcTransaction::Declare(RpcDeclareTransaction::V3(tx)) => {
                let ClassHashes { class_hash, executable_class_hash_v2 } =
                    self.class_manager_client.add_class(tx.contract_class).await?;
                // TODO(Aviv): Ensure that we do not want to
                // allow declare with compiled class hash v1.
                if tx.compiled_class_hash != executable_class_hash_v2 {
                    return Err(TransactionConverterError::ValidateCompiledClassHashError(
                        ValidateCompiledClassHashError::CompiledClassHashMismatch {
                            computed_class_hash: executable_class_hash_v2,
                            supplied_class_hash: tx.compiled_class_hash,
                        },
                    ));
                }
                InternalRpcTransactionWithoutTxHash::Declare(InternalRpcDeclareTransactionV3 {
                    sender_address: tx.sender_address,
                    compiled_class_hash: tx.compiled_class_hash,
                    signature: tx.signature,
                    nonce: tx.nonce,
                    class_hash,
                    resource_bounds: tx.resource_bounds,
                    tip: tx.tip,
                    paymaster_data: tx.paymaster_data,
                    account_deployment_data: tx.account_deployment_data,
                    nonce_data_availability_mode: tx.nonce_data_availability_mode,
                    fee_data_availability_mode: tx.fee_data_availability_mode,
                })
            }
            RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(tx)) => {
                let contract_address = tx.calculate_contract_address()?;
                InternalRpcTransactionWithoutTxHash::DeployAccount(
                    InternalRpcDeployAccountTransaction {
                        tx: RpcDeployAccountTransaction::V3(tx),
                        contract_address,
                    },
                )
            }
        };
        let tx_hash = tx_without_hash.calculate_transaction_hash(&self.chain_id)?;

        Ok(InternalRpcTransaction { tx: tx_without_hash, tx_hash })
    }

    async fn convert_internal_rpc_tx_to_executable_tx(
        &self,
        InternalRpcTransaction { tx, tx_hash }: InternalRpcTransaction,
    ) -> TransactionConverterResult<AccountTransaction> {
        match tx {
            InternalRpcTransactionWithoutTxHash::Invoke(tx) => {
                Ok(AccountTransaction::Invoke(executable_transaction::InvokeTransaction {
                    tx: tx.into(),
                    tx_hash,
                }))
            }
            InternalRpcTransactionWithoutTxHash::Declare(tx) => {
                let sierra = self.get_sierra(tx.class_hash).await?;
                let class_info = ClassInfo {
                    contract_class: self.get_executable(tx.class_hash).await?,
                    sierra_program_length: sierra.sierra_program.len(),
                    abi_length: sierra.abi.len(),
                    sierra_version: SierraVersion::extract_from_program(&sierra.sierra_program)?,
                };

                Ok(AccountTransaction::Declare(executable_transaction::DeclareTransaction {
                    tx: tx.into(),
                    tx_hash,
                    class_info,
                }))
            }
            InternalRpcTransactionWithoutTxHash::DeployAccount(
                InternalRpcDeployAccountTransaction { tx, contract_address },
            ) => Ok(AccountTransaction::DeployAccount(
                executable_transaction::DeployAccountTransaction {
                    tx: tx.into(),
                    contract_address,
                    tx_hash,
                },
            )),
        }
    }

    async fn convert_internal_consensus_tx_to_executable_tx(
        &self,
        tx: InternalConsensusTransaction,
    ) -> TransactionConverterResult<ExecutableTransaction> {
        match tx {
            InternalConsensusTransaction::RpcTransaction(tx) => Ok(ExecutableTransaction::Account(
                self.convert_internal_rpc_tx_to_executable_tx(tx).await?,
            )),
            InternalConsensusTransaction::L1Handler(tx) => Ok(ExecutableTransaction::L1Handler(tx)),
        }
    }
}

impl TransactionConverter {
    fn convert_consensus_l1_handler_to_internal_l1_handler(
        &self,
        tx: transaction::L1HandlerTransaction,
    ) -> TransactionConverterResult<executable_transaction::L1HandlerTransaction> {
        Ok(executable_transaction::L1HandlerTransaction::create(
            tx,
            &self.chain_id,
            // TODO(Gilad): Change this once we put real value in paid_fee_on_l1.
            Fee(1),
        )?)
    }
}
