use starknet_api::execution_resources::{GasAmount, GasVector};
use starknet_api::transaction::Resource::{self, L1DataGas, L1Gas, L2Gas};
use starknet_api::transaction::{AllResourceBounds, Fee, ResourceBounds, ValidResourceBounds};
use starknet_types_core::felt::Felt;
use thiserror::Error;

use crate::context::TransactionContext;
use crate::fee::fee_utils::{get_balance_and_if_covers_fee, get_fee_by_gas_vector};
use crate::fee::receipt::TransactionReceipt;
use crate::state::state_api::StateReader;
use crate::transaction::errors::TransactionExecutionError;
use crate::transaction::objects::{FeeType, TransactionExecutionResult, TransactionInfo};

#[derive(Clone, Copy, Debug, Error)]
pub enum FeeCheckError {
    #[error(
        "Insufficient max {resource}: max amount: {max_amount}, actual used: {actual_amount}."
    )]
    MaxGasAmountExceeded { resource: Resource, max_amount: GasAmount, actual_amount: GasAmount },
    #[error("Insufficient max fee: max fee: {}, actual fee: {}.", max_fee.0, actual_fee.0)]
    MaxFeeExceeded { max_fee: Fee, actual_fee: Fee },
    #[error(
        "Insufficient fee token balance. Fee: {}, balance: low/high \
         {balance_low}/{balance_high}.", fee.0
    )]
    InsufficientFeeTokenBalance { fee: Fee, balance_low: Felt, balance_high: Felt },
}

pub(crate) type FeeCheckResult<T> = Result<T, FeeCheckError>;

/// This struct holds the result of fee checks: recommended fee to charge (useful in post-execution
/// revert flow) and an error if the check failed.
struct FeeCheckReport {
    recommended_fee: Fee,
    error: Option<FeeCheckError>,
}

pub trait FeeCheckReportFields {
    fn recommended_fee(&self) -> Fee;
    fn error(&self) -> Option<FeeCheckError>;
}

impl FeeCheckReportFields for FeeCheckReport {
    fn recommended_fee(&self) -> Fee {
        self.recommended_fee
    }

    fn error(&self) -> Option<FeeCheckError> {
        self.error
    }
}

// TODO(Aner, 23/1/24): Update this struct to check data gas bounds as well as other bounds.
impl FeeCheckReport {
    pub fn success_report(actual_fee: Fee) -> Self {
        Self { recommended_fee: actual_fee, error: None }
    }

    /// Given a fee error and the current context, constructs and returns a report.
    pub fn from_fee_check_error(
        actual_fee: Fee,
        error: FeeCheckError,
        tx_context: &TransactionContext,
    ) -> Self {
        let recommended_fee = match error {
            // If the error is insufficient balance, the recommended fee is the actual fee.
            // This recommendation assumes (a) the pre-validation checks were applied and pass (i.e.
            // the sender initially could cover the resource bounds), and (b) the actual resources
            // are within the resource bounds set by the sender.
            FeeCheckError::InsufficientFeeTokenBalance { .. } => actual_fee,
            // If the error is resource overdraft, the recommended fee is the resource bounds.
            // If the transaction passed pre-validation checks (i.e. balance initially covered the
            // resource bounds), the sender should be able to pay this fee.
            FeeCheckError::MaxFeeExceeded { .. } | FeeCheckError::MaxGasAmountExceeded { .. } => {
                match &tx_context.tx_info {
                    TransactionInfo::Current(info) => get_fee_by_gas_vector(
                        &tx_context.block_context.block_info,
                        GasVector::from_l1_gas(info.l1_resource_bounds().max_amount.into()),
                        &FeeType::Strk,
                    ),
                    TransactionInfo::Deprecated(context) => context.max_fee,
                }
            }
        };
        Self { recommended_fee, error: Some(error) }
    }

    /// If the actual cost exceeds the resource bounds on the transaction, returns a fee check
    /// error.
    fn check_actual_cost_within_bounds(
        tx_context: &TransactionContext,
        tx_receipt: &TransactionReceipt,
    ) -> TransactionExecutionResult<()> {
        let TransactionReceipt { fee, gas, .. } = tx_receipt;
        let TransactionContext { tx_info, .. } = tx_context;

        // First, compare the actual resources used against the upper bound(s) defined by the
        // sender.
        match tx_info {
            TransactionInfo::Current(context) => Ok(FeeCheckReport::check_resources_within_bounds(
                &context.resource_bounds,
                gas,
                tx_context,
            )?),
            TransactionInfo::Deprecated(context) => {
                // Check max fee.
                let max_fee = context.max_fee;
                if fee > &max_fee {
                    return Err(FeeCheckError::MaxFeeExceeded { max_fee, actual_fee: *fee })?;
                }
                Ok(())
            }
        }
    }

    /// If the actual cost exceeds the sender's balance, returns a fee check error.
    fn check_can_pay_fee<S: StateReader>(
        state: &mut S,
        tx_context: &TransactionContext,
        tx_receipt: &TransactionReceipt,
    ) -> TransactionExecutionResult<()> {
        let TransactionReceipt { fee, .. } = *tx_receipt;
        let (balance_low, balance_high, can_pay) =
            get_balance_and_if_covers_fee(state, tx_context, fee)?;
        if can_pay {
            return Ok(());
        }
        Err(FeeCheckError::InsufficientFeeTokenBalance { fee, balance_low, balance_high })?
    }

    /// Checks that the actual resources used are within the bounds set by the sender.
    fn check_resources_within_bounds(
        valid_resource_bounds: &ValidResourceBounds,
        gas_vector: &GasVector,
        // TODO(Aviv): delete the tx_context parameter.
        tx_context: &TransactionContext,
    ) -> FeeCheckResult<()> {
        match valid_resource_bounds {
            ValidResourceBounds::AllResources(all_resource_bounds) => {
                // Iterate over resources and check actual_amount <= max_amount.
                FeeCheckReport::check_all_resources_within_bounds(all_resource_bounds, gas_vector)
            }
            ValidResourceBounds::L1Gas(l1_bounds) => {
                // Check that the total discounted l1 gas used <= l1_bounds.max_amount.
                FeeCheckReport::check_l1_gas_within_bounds(l1_bounds, gas_vector, tx_context)
            }
        }
    }

    fn check_all_resources_within_bounds(
        all_resource_bounds: &AllResourceBounds,
        gas_vector: &GasVector,
    ) -> FeeCheckResult<()> {
        for (resource, max_amount, actual_amount) in [
            (L1Gas, all_resource_bounds.l1_gas.max_amount.into(), gas_vector.l1_gas),
            (L2Gas, all_resource_bounds.l2_gas.max_amount.into(), gas_vector.l2_gas),
            (L1DataGas, all_resource_bounds.l1_data_gas.max_amount.into(), gas_vector.l1_data_gas),
        ] {
            if max_amount < actual_amount {
                return Err(FeeCheckError::MaxGasAmountExceeded {
                    resource,
                    max_amount,
                    actual_amount,
                });
            }
        }

        Ok(())
    }

    fn check_l1_gas_within_bounds(
        &l1_bounds: &ResourceBounds,
        gas_vector: &GasVector,
        tx_context: &TransactionContext,
    ) -> FeeCheckResult<()> {
        let total_discounted_gas_used =
            gas_vector.to_discounted_l1_gas(tx_context.get_gas_prices());
        if total_discounted_gas_used > l1_bounds.max_amount.into() {
            return Err(FeeCheckError::MaxGasAmountExceeded {
                resource: L1Gas,
                max_amount: l1_bounds.max_amount.into(),
                actual_amount: total_discounted_gas_used,
            });
        }
        Ok(())
    }
}

macro_rules! impl_report_fields {
    ($report_type:ty) => {
        impl FeeCheckReportFields for $report_type {
            fn recommended_fee(&self) -> Fee {
                self.0.recommended_fee()
            }

            fn error(&self) -> Option<FeeCheckError> {
                self.0.error()
            }
        }
    };
}

pub struct PostValidationReport(FeeCheckReport);
pub struct PostExecutionReport(FeeCheckReport);

impl_report_fields!(PostValidationReport);
impl_report_fields!(PostExecutionReport);

impl PostValidationReport {
    /// Verifies that the actual cost of validation is within sender bounds.
    /// Note: the balance cannot be changed in `__validate__` (which cannot call other contracts),
    /// so there is no need to recheck that balance >= actual cost.
    pub fn verify(
        tx_context: &TransactionContext,
        tx_receipt: &TransactionReceipt,
    ) -> TransactionExecutionResult<()> {
        // If fee is not enforced, no need to check post-execution.
        if !tx_context.tx_info.enforce_fee() {
            return Ok(());
        }

        FeeCheckReport::check_actual_cost_within_bounds(tx_context, tx_receipt)
    }
}

impl PostExecutionReport {
    /// Verifies the actual cost can be paid by the account. If not, reports an error and the fee
    /// that should be charged in revert flow.
    pub fn new<S: StateReader>(
        state: &mut S,
        tx_context: &TransactionContext,
        tx_receipt: &TransactionReceipt,
        charge_fee: bool,
    ) -> TransactionExecutionResult<Self> {
        let TransactionReceipt { fee, .. } = tx_receipt;

        // If fee is not enforced, no need to check post-execution.
        if !charge_fee || !tx_context.tx_info.enforce_fee() {
            return Ok(Self(FeeCheckReport::success_report(*fee)));
        }

        // First, compare the actual resources used against the upper bound(s) defined by the
        // sender.
        let cost_with_bounds_result =
            FeeCheckReport::check_actual_cost_within_bounds(tx_context, tx_receipt);

        // Next, verify the actual cost is covered by the account balance, which may have changed
        // after execution. If the above check passes, the pre-execution balance covers the actual
        // cost for sure.
        let can_pay_fee_result = FeeCheckReport::check_can_pay_fee(state, tx_context, tx_receipt);

        for fee_check_result in [cost_with_bounds_result, can_pay_fee_result] {
            match fee_check_result {
                Ok(_) => continue,
                Err(TransactionExecutionError::FeeCheckError(fee_check_error)) => {
                    // Found an error; set the recommended fee based on the error variant and
                    // current context, and return the report.
                    return Ok(Self(FeeCheckReport::from_fee_check_error(
                        *fee,
                        fee_check_error,
                        tx_context,
                    )));
                }
                Err(other_error) => return Err(other_error),
            }
        }

        Ok(Self(FeeCheckReport::success_report(*fee)))
    }
}
