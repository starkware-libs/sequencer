use std::time::{Duration, Instant};

use apollo_class_manager_types::{ClassHashes, ClassManagerClientError, SharedClassManagerClient};
use apollo_proof_manager_types::{ProofManagerClientError, SharedProofManagerClient};
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
    RpcInvokeTransaction,
    RpcInvokeTransactionV3,
    RpcTransaction,
};
use starknet_api::state::SierraContractClass;
use starknet_api::transaction::fields::{Fee, Proof, ProofFacts};
use starknet_api::transaction::{CalculateContractAddress, TransactionHash};
use starknet_api::{executable_transaction, transaction, StarknetApiError};
use starknet_proof_verifier::VerifyProofError;
use starknet_types_core::felt::Felt;
use thiserror::Error;
use tokio::task::JoinHandle;
use tracing::info;

use crate::metrics::{
    ComponentLabelValue,
    CONSENSUS_PROOF_MANAGER_STORE_LATENCY,
    LABEL_NAME_COMPONENT,
    PROOF_VERIFICATION_COUNT,
    PROOF_VERIFICATION_LATENCY,
};

#[cfg(test)]
#[path = "transaction_converter_test.rs"]
pub mod transaction_converter_test;

#[derive(Error, Debug, PartialEq)]
pub enum TransactionConverterError {
    #[error(transparent)]
    ClassManagerClientError(#[from] ClassManagerClientError),
    #[error("Class of hash: {class_hash} not found")]
    ClassNotFound { class_hash: ClassHash },
    #[error("Proof for proof facts hash: {facts_hash} not found.")]
    ProofNotFound { facts_hash: Felt },
    #[error(transparent)]
    ProofManagerClientError(#[from] ProofManagerClientError),
    #[error(transparent)]
    ProofVerificationError(#[from] VerifyProofError),
    #[error(transparent)]
    StarknetApiError(#[from] StarknetApiError),
    #[error(transparent)]
    ValidateCompiledClassHashError(#[from] ValidateCompiledClassHashError),
}

pub type TransactionConverterResult<T> = Result<T, TransactionConverterError>;

pub type VerifyAndStoreProofTask = JoinHandle<Result<(), TransactionConverterError>>;

#[derive(Debug)]
pub struct VerificationHandle {
    // TODO(Dori): add a field for the class hash.
    pub proof_facts: ProofFacts,
    pub proof: Proof,
    pub verification_task: JoinHandle<Result<(), TransactionConverterError>>,
}

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
    ) -> TransactionConverterResult<(InternalConsensusTransaction, Option<VerifyAndStoreProofTask>)>;

    async fn convert_internal_rpc_tx_to_rpc_tx(
        &self,
        tx: InternalRpcTransaction,
    ) -> TransactionConverterResult<RpcTransaction>;

    async fn convert_rpc_tx_to_internal_rpc_tx(
        &self,
        tx: RpcTransaction,
    ) -> TransactionConverterResult<(InternalRpcTransaction, Option<VerificationHandle>)>;

    async fn convert_internal_rpc_tx_to_executable_tx(
        &self,
        tx: InternalRpcTransaction,
    ) -> TransactionConverterResult<AccountTransaction>;

    async fn convert_internal_consensus_tx_to_executable_tx(
        &self,
        tx: InternalConsensusTransaction,
    ) -> TransactionConverterResult<ExecutableTransaction>;

    async fn store_proof_in_proof_manager(
        &self,
        proof_facts: ProofFacts,
        tx_hash: TransactionHash,
        proof: Proof,
    ) -> TransactionConverterResult<Duration>;
}

#[derive(Clone)]
pub struct TransactionConverter {
    class_manager_client: SharedClassManagerClient,
    proof_manager_client: SharedProofManagerClient,
    chain_id: ChainId,
}

impl TransactionConverter {
    pub fn new(
        class_manager_client: SharedClassManagerClient,
        proof_manager_client: SharedProofManagerClient,
        chain_id: ChainId,
    ) -> Self {
        Self { class_manager_client, proof_manager_client, chain_id }
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

    async fn get_proof(
        &self,
        proof_facts: &ProofFacts,
        tx_hash: TransactionHash,
    ) -> TransactionConverterResult<Proof> {
        let start_time = Instant::now();
        let proof_facts_hash = proof_facts.hash();
        let proof = self
            .proof_manager_client
            .get_proof(proof_facts.clone(), tx_hash)
            .await?
            .ok_or(TransactionConverterError::ProofNotFound { facts_hash: proof_facts_hash });
        let duration = start_time.elapsed();
        info!(
            "Getting the proof from the proof manager took: {duration:?} for proof facts hash: \
             {proof_facts_hash:?}"
        );
        proof
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
    ) -> TransactionConverterResult<(InternalConsensusTransaction, Option<VerifyAndStoreProofTask>)>
    {
        match tx {
            ConsensusTransaction::RpcTransaction(tx) => {
                let (internal_tx, proof_data) = self.convert_rpc_tx_to_internal(tx).await?;
                let task = proof_data.map(|(proof_facts, proof)| {
                    self.spawn_verify_and_store_proof(proof_facts, proof, internal_tx.tx_hash)
                });
                Ok((InternalConsensusTransaction::RpcTransaction(internal_tx), task))
            }
            ConsensusTransaction::L1Handler(tx) => {
                let internal_tx = self.convert_consensus_l1_handler_to_internal_l1_handler(tx)?;
                Ok((InternalConsensusTransaction::L1Handler(internal_tx), None))
            }
        }
    }

    async fn convert_internal_rpc_tx_to_rpc_tx(
        &self,
        tx: InternalRpcTransaction,
    ) -> TransactionConverterResult<RpcTransaction> {
        let InternalRpcTransaction { tx: tx_without_hash, tx_hash } = tx;
        match tx_without_hash {
            InternalRpcTransactionWithoutTxHash::Invoke(tx) => {
                // We expect the proof to be available here because it has already been verified
                // and stored by the proof manager in the gateway.
                let proof = if tx.proof_facts.is_empty() {
                    Proof::default()
                } else {
                    self.get_proof(&tx.proof_facts, tx_hash).await?
                };

                Ok(RpcTransaction::Invoke(RpcInvokeTransaction::V3(RpcInvokeTransactionV3 {
                    resource_bounds: tx.resource_bounds,
                    signature: tx.signature,
                    nonce: tx.nonce,
                    tip: tx.tip,
                    paymaster_data: tx.paymaster_data,
                    nonce_data_availability_mode: tx.nonce_data_availability_mode,
                    fee_data_availability_mode: tx.fee_data_availability_mode,
                    sender_address: tx.sender_address,
                    calldata: tx.calldata,
                    account_deployment_data: tx.account_deployment_data,
                    proof_facts: tx.proof_facts,
                    proof,
                })))
            }
            InternalRpcTransactionWithoutTxHash::Declare(tx) => {
                Ok(RpcTransaction::Declare(RpcDeclareTransaction::V3(RpcDeclareTransactionV3 {
                    sender_address: tx.sender_address,
                    compiled_class_hash: tx.compiled_class_hash,
                    signature: tx.signature,
                    nonce: tx.nonce,
                    // We expect the sierra to be available here because it has already been added
                    // to the class manager in the gateway.
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
    ) -> TransactionConverterResult<(InternalRpcTransaction, Option<VerificationHandle>)> {
        let (internal_tx, proof_data) = self.convert_rpc_tx_to_internal(tx).await?;
        let verification_handle = proof_data
            .map(|(proof_facts, proof)| {
                self.spawn_proof_verification(proof_facts, proof, internal_tx.tx_hash)
            })
            .transpose()?;
        Ok((internal_tx, verification_handle))
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
                let (sierra, contract_class) = tokio::try_join!(
                    self.get_sierra(tx.class_hash),
                    self.get_executable(tx.class_hash)
                )?;
                let class_info = ClassInfo {
                    contract_class,
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

    async fn store_proof_in_proof_manager(
        &self,
        proof_facts: ProofFacts,
        tx_hash: TransactionHash,
        proof: Proof,
    ) -> TransactionConverterResult<Duration> {
        let start = Instant::now();
        self.proof_manager_client.set_proof(proof_facts, tx_hash, proof).await?;
        Ok(start.elapsed())
    }
}

impl TransactionConverter {
    /// Converts an RPC transaction to its internal representation without spawning any proof tasks.
    /// Returns the proof data (if present) separately so each caller can decide how to handle it.
    async fn convert_rpc_tx_to_internal(
        &self,
        tx: RpcTransaction,
    ) -> TransactionConverterResult<(InternalRpcTransaction, Option<(ProofFacts, Proof)>)> {
        let (tx_without_hash, proof_data) = match tx {
            RpcTransaction::Invoke(RpcInvokeTransaction::V3(tx)) => {
                let proof_data = if tx.proof_facts.is_empty() {
                    None
                } else {
                    Some((tx.proof_facts.clone(), tx.proof.clone()))
                };
                (InternalRpcTransactionWithoutTxHash::Invoke(tx.into()), proof_data)
            }
            RpcTransaction::Declare(RpcDeclareTransaction::V3(tx)) => {
                let ClassHashes { class_hash, executable_class_hash_v2 } =
                // TODO(Dori): Make this async and spawn a task to compile and add it to the class manager.
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
                (
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
                    }),
                    None,
                )
            }
            RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(tx)) => {
                let contract_address = tx.calculate_contract_address()?;
                (
                    InternalRpcTransactionWithoutTxHash::DeployAccount(
                        InternalRpcDeployAccountTransaction {
                            tx: RpcDeployAccountTransaction::V3(tx),
                            contract_address,
                        },
                    ),
                    None,
                )
            }
        };
        let tx_hash = tx_without_hash.calculate_transaction_hash(&self.chain_id)?;
        Ok((InternalRpcTransaction { tx: tx_without_hash, tx_hash }, proof_data))
    }

    /// Runs proof verification: checks if the proof already exists, and if not, verifies it.
    /// Returns `true` if verification was performed, `false` if skipped (proof already stored).
    /// This is the shared verification logic used by both gateway and consensus flows.
    async fn run_proof_verification(
        proof_facts: ProofFacts,
        proof: Proof,
        proof_manager_client: SharedProofManagerClient,
        tx_hash: TransactionHash,
        component: ComponentLabelValue,
    ) -> Result<bool, TransactionConverterError> {
        let contains_proof =
            proof_manager_client.contains_proof(proof_facts.clone(), tx_hash).await?;

        if contains_proof {
            return Ok(false);
        }

        let proof_facts_hash = proof_facts.hash();
        let verify_start = Instant::now();
        tokio::task::spawn_blocking(move || {
            starknet_proof_verifier::verify_proof(proof_facts, proof)
        })
        .await
        .expect("proof verification task panicked")?;
        let verify_duration = verify_start.elapsed();
        PROOF_VERIFICATION_LATENCY.record(verify_duration.as_secs_f64());
        PROOF_VERIFICATION_COUNT.increment(1, &[(LABEL_NAME_COMPONENT, component.into())]);
        info!(
            "Proof verification took: {verify_duration:?} for proof facts hash: \
             {proof_facts_hash:?}"
        );

        Ok(true)
    }

    /// Spawns a verification-only task. Used by the gateway flow, which stores the proof
    /// separately after all validations pass.
    fn spawn_proof_verification(
        &self,
        proof_facts: ProofFacts,
        proof: Proof,
        tx_hash: TransactionHash,
    ) -> TransactionConverterResult<VerificationHandle> {
        let pmc = self.proof_manager_client.clone();
        let task_proof_facts = proof_facts.clone();
        let task_proof = proof.clone();
        let verification_task = tokio::spawn(async move {
            Self::run_proof_verification(
                task_proof_facts,
                task_proof,
                pmc,
                tx_hash,
                ComponentLabelValue::Gateway,
            )
            .await?;
            Ok(())
        });
        Ok(VerificationHandle { proof_facts, proof, verification_task })
    }

    /// Spawns a single task that verifies the proof and then stores it in the proof manager.
    /// Used by the consensus flow, where tasks run concurrently with batcher execution and
    /// are awaited at fin.
    fn spawn_verify_and_store_proof(
        &self,
        proof_facts: ProofFacts,
        proof: Proof,
        tx_hash: TransactionHash,
    ) -> VerifyAndStoreProofTask {
        let pmc = self.proof_manager_client.clone();
        let proof_facts_hash = proof_facts.hash();
        tokio::spawn(async move {
            let verified = Self::run_proof_verification(
                proof_facts.clone(),
                proof.clone(),
                pmc.clone(),
                tx_hash,
                ComponentLabelValue::Consensus,
            )
            .await?;

            if !verified {
                return Ok(());
            }

            let start = Instant::now();
            pmc.set_proof(proof_facts, tx_hash, proof).await?;
            let duration = start.elapsed();
            CONSENSUS_PROOF_MANAGER_STORE_LATENCY.record(duration.as_secs_f64());
            info!(
                "Proof manager store took: {duration:?} for proof facts hash: {proof_facts_hash:?}"
            );
            Ok(())
        })
    }

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
