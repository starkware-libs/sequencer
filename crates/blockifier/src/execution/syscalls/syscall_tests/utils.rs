use crate::execution::call_info::CallInfo;
use crate::execution::entry_point::EntryPointExecutionResult;
#[cfg(feature = "cairo_native")]
use crate::execution::native::utils::decode_felts_as_str;
use crate::test_utils::contracts::FeatureContract;
#[cfg(feature = "cairo_native")]
use crate::test_utils::CairoVersion;

pub fn get_error_message(
    #[allow(unused_variables)] test_contract: FeatureContract,
    call_result: EntryPointExecutionResult<CallInfo>,
) -> String {
    #[cfg(feature = "cairo_native")]
    let error_message =
        if matches!(test_contract, FeatureContract::TestContract(CairoVersion::Native)) {
            let call_info = call_result
                .expect("Expected CallResult with failed execution error message in the retdata.");
            decode_felts_as_str(&call_info.execution.retdata.0)
        } else {
            call_result.unwrap_err().to_string()
        };
    #[cfg(not(feature = "cairo_native"))]
    let error_message = call_result.unwrap_err().to_string();

    error_message
}
