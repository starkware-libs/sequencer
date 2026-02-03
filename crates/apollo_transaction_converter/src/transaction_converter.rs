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
use starknet_api::transaction::fields::{Fee, Proof, ProofFacts, PROOF_VERSION};
use starknet_api::transaction::CalculateContractAddress;
use starknet_api::{executable_transaction, transaction, StarknetApiError};
use starknet_types_core::felt::Felt;
use thiserror::Error;
use tokio::task::JoinHandle;
use tracing::info;

use crate::proof_verification::{stwo_verify, VerifyProofError};

/// The expected bootloader program hash for proof verification.
pub const BOOTLOADER_PROGRAM_HASH: Felt =
    Felt::from_hex_unchecked("0x3faf9fbac01a844107ca8f272e78763d3818ac40ed9107307271b651e7efe0d");

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

pub type VerificationTask = JoinHandle<Result<(), TransactionConverterError>>;

#[derive(Debug)]
pub struct VerificationHandle {
    // TODO(Dori): add a field for the class hash.
    pub proof_facts: ProofFacts,
    pub proof: Proof,
    pub verification_task: VerificationTask,
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
    ) -> TransactionConverterResult<(InternalConsensusTransaction, Option<VerificationHandle>)>;

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

    fn get_proof_manager_client(&self) -> SharedProofManagerClient;

    async fn store_proof_in_proof_manager(
        &self,
        proof_facts: ProofFacts,
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

    async fn get_proof(&self, proof_facts: &ProofFacts) -> TransactionConverterResult<Proof> {
        self.proof_manager_client
            .get_proof(proof_facts.clone())
            .await?
            .ok_or(TransactionConverterError::ProofNotFound { facts_hash: proof_facts.hash() })
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
    ) -> TransactionConverterResult<(InternalConsensusTransaction, Option<VerificationHandle>)>
    {
        match tx {
            ConsensusTransaction::RpcTransaction(tx) => {
                let (internal_tx, verification_handle) =
                    self.convert_rpc_tx_to_internal_rpc_tx(tx).await?;
                Ok((InternalConsensusTransaction::RpcTransaction(internal_tx), verification_handle))
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
        match tx.tx {
            InternalRpcTransactionWithoutTxHash::Invoke(tx) => {
                // We expect the proof to be available here because it has already been verified
                // and stored by the proof manager in the gateway.
                let proof = if tx.proof_facts.is_empty() {
                    Proof::default()
                } else {
                    self.get_proof(&tx.proof_facts).await?
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
        let (tx_without_hash, verification_handle) = match tx {
            RpcTransaction::Invoke(RpcInvokeTransaction::V3(tx)) => {
                // Spawn proof verification task; storage happens in the caller after successful
                // conversion/validation.
                let verification_handle =
                    self.spawn_proof_verification(&tx.proof_facts, &tx.proof)?;
                (InternalRpcTransactionWithoutTxHash::Invoke(tx.into()), verification_handle)
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

        Ok((InternalRpcTransaction { tx: tx_without_hash, tx_hash }, verification_handle))
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

    fn get_proof_manager_client(&self) -> SharedProofManagerClient {
        self.proof_manager_client.clone()
    }
    async fn store_proof_in_proof_manager(
        &self,
        proof_facts: ProofFacts,
        proof: Proof,
    ) -> TransactionConverterResult<Duration> {
        let proof_manager_client = self.proof_manager_client.clone();
        let start = Instant::now();
        proof_manager_client.set_proof(proof_facts, proof).await?;
        Ok(start.elapsed())
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

    fn spawn_proof_verification(
        &self,
        proof_facts: &ProofFacts,
        proof: &Proof,
    ) -> TransactionConverterResult<Option<VerificationHandle>> {
        // If the proof facts are empty, it is a standard transaction that does not use client-side
        // proving and we skip proof verification. We return Ok only if the proof facts are
        // empty (and not the proof) because the proof facts are trusted and we do not want
        // transactions that are missing proofs but contain proof facts to be accepted.
        if proof_facts.is_empty() {
            return Ok(None);
        }

        // Clone data needed for the spawned task.
        let proof_facts_for_task = proof_facts.clone();
        let proof_for_task = proof.clone();
        // Clone the proof manager client so it can be safely moved into the spawned async task
        // without lifetime or ownership issues.
        let proof_manager_client = self.proof_manager_client.clone();

        // Spawn verification task.
        let verification_task = tokio::spawn(async move {
            let contains_proof =
                proof_manager_client.contains_proof(proof_facts_for_task.clone()).await?;

            // If the proof already exists in the proof manager, indicating it has already been
            // verified, we skip proof verification.
            if contains_proof {
                return Ok(());
            }

            let verify_start = Instant::now();
            Self::verify_proof(proof_facts_for_task.clone(), proof_for_task)?;
            let verify_duration = verify_start.elapsed();
            let proof_facts_hash = proof_facts_for_task.hash();
            info!(
                "Proof verification took: {verify_duration:?} for proof facts hash: \
                 {proof_facts_hash:?}"
            );

            Ok(())
        });

        Ok(Some(VerificationHandle {
            proof_facts: proof_facts.clone(),
            proof: proof.clone(),
            verification_task,
        }))
    }

    /// Verifies a submitted proof, validating the emitted proof facts, and comparing the bootloader
    /// program hash to the expected value.
    fn verify_proof(proof_facts: ProofFacts, proof: Proof) -> Result<(), VerifyProofError> {
        // Reject empty proof payloads before running the verifier.
        if proof.is_empty() {
            return Err(VerifyProofError::EmptyProof);
        }

        // Validate that the first element of proof facts is PROOF_VERSION.
        let expected_proof_version = PROOF_VERSION;
        let actual_first = proof_facts.0.first().copied().unwrap_or_default();
        if actual_first != expected_proof_version {
            return Err(VerifyProofError::InvalidProofVersion {
                expected: expected_proof_version,
                actual: actual_first,
            });
        }

        // Verify proof and extract program output and program hash.
        let output = stwo_verify(proof)?;

        let program_variant = proof_facts.0.get(1).copied().unwrap_or_default();
        let expected_proof_facts = output.program_output.try_into_proof_facts(program_variant)?;
        if expected_proof_facts != proof_facts {
            return Err(VerifyProofError::ProofFactsMismatch);
        }

        // Validate the bootloader program hash output against the expected bootloader hash.
        if output.program_hash != BOOTLOADER_PROGRAM_HASH {
            return Err(VerifyProofError::BootloaderHashMismatch);
        }

        Ok(())
    }
}
