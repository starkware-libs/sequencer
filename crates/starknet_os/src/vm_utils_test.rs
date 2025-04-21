use std::collections::HashMap;

use cairo_vm::serde::deserialize_program::Identifier;
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::vm_core::VirtualMachine;
use rstest::rstest;
use serde_json;

use super::{
    fetch_nested_fields_address,
    get_size_of_cairo_struct,
    IdentifierGetter,
    VmUtilsResult,
};
use crate::hints::vars::CairoStruct;

impl IdentifierGetter for HashMap<String, Identifier> {
    fn get_identifier(&self, identifier_name: &str) -> VmUtilsResult<&Identifier> {
        Ok(self.get(identifier_name).unwrap())
    }
}

#[rstest]
#[case::depth_1(0, vec!["double_double_point"])]
#[case::depth_2(4, vec!["double_double_point", "double_point2"])]
#[case::depth_2(6, vec!["double_double_point", "double_point2", "point2"])]
#[case::depth_4(3, vec!["double_double_point", "double_point1", "point2", "y"])]
fn get_address_of_nested_fields_without_ptrs(
    #[case] expected_offset: usize,
    #[case] nested_fields: Vec<&str>,
) {
    let identifiers_json = r#"
    {
        "starkware.cairo.common.ec_point.EcPoint": {
            "full_name": "starkware.cairo.common.ec_point.EcPoint",
            "members": {
                "x": {
                    "cairo_type": "felt",
                    "offset": 0
                },
                "y": {
                    "cairo_type": "felt",
                    "offset": 1
                }
            },
            "size": 2,
            "type": "struct"
        },
        "DoublePoint": {
            "full_name": "DoublePoint",
            "members": {
                "point1": {
                    "cairo_type": "starkware.cairo.common.ec_point.EcPoint",
                    "offset": 0
                },
                "point2": {
                    "cairo_type": "starkware.cairo.common.ec_point.EcPoint",
                    "offset": 2
                }
            },
            "size": 4,
            "type": "struct"
        },
        "DoubleDoublePoint": {
            "full_name": "DoubleDoublePoint",
            "members": {
                "double_point1": {
                    "cairo_type": "DoublePoint",
                    "offset": 0
                },
                "double_point2": {
                    "cairo_type": "DoublePoint",
                    "offset": 4
                }
            },
            "size": 8,
            "type": "struct"
        },
        "DoubleDoublePointWrapper": {
            "full_name": "DoubleDoublePointWrapper",
            "members": {
                "double_double_point": {
                    "cairo_type": "DoubleDoublePoint",
                    "offset": 0
                }
            },
            "size": 8,
            "type": "struct"
        }

    }"#;

    let identifiers: HashMap<String, Identifier> = serde_json::from_str(identifiers_json).unwrap();
    let trace_enabled = false;
    let disable_trace_padding = false;
    let vm = VirtualMachine::new(trace_enabled, disable_trace_padding); // Dummy VM.
    let dummy_base_address = Relocatable::from((11, 48)); // This is fetchable from 'wrapper'.
    let base_struct = identifiers.get("DoubleDoublePointWrapper").unwrap();
    let actual_base_address = fetch_nested_fields_address(
        dummy_base_address,
        base_struct,
        &nested_fields,
        &identifiers,
        &vm,
    )
    .unwrap();
    assert_eq!(actual_base_address, (dummy_base_address + expected_offset).unwrap())
}

// TODO(Nimrod): Add test cases with pointers.

#[rstest]
#[case::casm_contract_entry_point(4, CairoStruct::CompiledClassEntryPoint)]
#[case::compiled_class_fact(2, CairoStruct::CompiledClassFact)]
#[case::deprecated_contract_entry_point(2, CairoStruct::DeprecatedContractEntryPoint)]
#[case::dict_access(3, CairoStruct::DictAccess)]
#[case::resource_as_felts(3, CairoStruct::ResourceBounds)]
fn size_of_cairo_structs(#[case] expected_size: usize, #[case] cairo_struct: CairoStruct) {
    let identifiers_json = r#"
    {
    "starkware.starknet.core.os.contract_class.compiled_class.CompiledClassEntryPoint": {
                "full_name": "starkware.starknet.core.os.contract_class.compiled_class.CompiledClassEntryPoint",
                "members": {
                    "selector": {
                        "cairo_type": "felt",
                        "offset": 0
                    },
                    "offset": {
                        "cairo_type": "felt",
                        "offset": 1
                    },
                    "n_builtins": {
                        "cairo_type": "felt",
                        "offset": 2
                    },
                        "builtin_list": {
                        "cairo_type": "felt*",
                        "offset": 3
                    }
                },
                "size": 4,
                "type": "struct"
            },
    "starkware.starknet.core.os.contract_class.compiled_class.CompiledClassFact": {
                "full_name": "starkware.starknet.core.os.contract_class.compiled_class.CompiledClassFact",
                "members": {
                    "hash": {
                        "cairo_type": "felt",
                        "offset": 0
                    },
                    "compiled_class": {
                        "cairo_type": "CompiledClass",
                        "offset": 1
                    }
                },
                "size": 2,
                "type": "struct"
            },
    "starkware.starknet.core.os.contract_class.deprecated_compiled_class.DeprecatedContractEntryPoint": {
                "full_name": "starkware.starknet.core.os.contract_class.deprecated_compiled_class.DeprecatedContractEntryPoint",
                "members": {
                    "selector": {
                        "cairo_type": "felt",
                        "offset": 0
                    },
                    "offset": {
                        "cairo_type": "felt",
                        "offset": 1
                    }
                },
                "size": 2,
                "type": "struct"
            },
    "starkware.cairo.common.dict_access.DictAccess": {
                "full_name": "starkware.cairo.common.dict_access.DictAccess",
                "members": {
                    "key": {
                        "cairo_type": "felt",
                        "offset": 0
                    },
                    "new_value": {
                        "cairo_type": "felt",
                        "offset": 2
                    },
                    "prev_value": {
                        "cairo_type": "felt",
                        "offset": 1
                    }
                },
                "size": 3,
                "type": "struct"
            },
    "starkware.starknet.common.new_syscalls.ResourceBounds": {
                "full_name": "starkware.starknet.common.new_syscalls.ResourceBounds",
                "members": {
                    "resource": {
                        "cairo_type": "felt",
                        "offset": 0
                    },
                    "max_amount": {
                        "cairo_type": "felt",
                        "offset": 1
                    },
                    "max_price_per_unit": {
                        "cairo_type": "felt",
                        "offset": 2
                    }
                },
                "size": 3,
                "type": "struct"
            }
    }"#;

    let identifiers: HashMap<String, Identifier> = serde_json::from_str(identifiers_json).unwrap();
    let size = get_size_of_cairo_struct(cairo_struct, &identifiers).unwrap();
    assert_eq!(size, expected_size)
}
