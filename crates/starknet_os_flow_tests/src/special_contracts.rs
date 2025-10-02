use std::sync::LazyLock;

use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;

pub(crate) static V1_BOUND_CAIRO0_CONTRACT: LazyLock<DeprecatedContractClass> =
    LazyLock::new(|| {
        serde_json::from_str(include_str!("../resources/v1_bound_cairo0_account.json")).unwrap()
    });
