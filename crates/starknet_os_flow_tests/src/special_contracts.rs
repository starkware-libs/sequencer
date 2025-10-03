use std::sync::LazyLock;

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::SierraContractClass;

pub(crate) static V1_BOUND_CAIRO0_CONTRACT: LazyLock<DeprecatedContractClass> =
    LazyLock::new(|| {
        serde_json::from_str(include_str!("../resources/v1_bound_cairo0_account.json")).unwrap()
    });

pub(crate) static V1_BOUND_CAIRO1_CONTRACT_SIERRA: LazyLock<SierraContractClass> =
    LazyLock::new(|| {
        let compiler_contract_class: cairo_lang_starknet_classes::contract_class::ContractClass =
            serde_json::from_str(include_str!("../resources/v1_bound_cairo1_account.sierra.json"))
                .unwrap();
        SierraContractClass::from(compiler_contract_class)
    });

pub(crate) static V1_BOUND_CAIRO1_CONTRACT_CASM: LazyLock<CasmContractClass> =
    LazyLock::new(|| {
        serde_json::from_str(include_str!("../resources/v1_bound_cairo1_account.casm.json"))
            .unwrap()
    });
