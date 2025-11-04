use std::collections::HashMap;
use std::sync::LazyLock;

use apollo_starknet_os_program::AGGREGATOR_PROGRAM;
use ark_bls12_381::Fr;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::vm::runners::cairo_pie::{
    BuiltinAdditionalData,
    OutputBuiltinAdditionalData,
    PublicMemoryPage,
};
use itertools::Itertools;
use num_bigint::BigUint;
use num_integer::Integer;
use num_traits::ToPrimitive;
use rstest::rstest;
use starknet_api::core::{ChainId, ClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::{Poseidon, StarkHash};
use tempfile::NamedTempFile;

use crate::hint_processor::aggregator_hint_processor::{
    AggregatorHintProcessor,
    AggregatorInput,
    DataAvailability,
};
use crate::hints::hint_implementation::kzg::utils::{
    polynomial_coefficients_to_kzg_commitment,
    BLS_PRIME,
};
use crate::hints::hint_implementation::output::{MAX_PAGE_SIZE, OUTPUT_ATTRIBUTE_FACT_TOPOLOGY};
use crate::hints::hint_implementation::stateless_compression::utils::compress;
use crate::io::os_input::OsChainInfo;
use crate::io::os_output_types::{
    FullContractChanges,
    FullContractStorageUpdate,
    N_UPDATES_SMALL_PACKING_BOUND,
};
use crate::runner::{run_program, RunnerReturnObject};
use crate::test_utils::validations::validate_builtins;

// Dummy values for the test.
static OS_CONFIG_HASH: LazyLock<Felt> = LazyLock::new(|| {
    OsChainInfo {
        chain_id: ChainId::Other("\0".to_string()),
        strk_fee_token_address: ContractAddress::default(),
    }
    .compute_os_config_hash(None)
    .unwrap()
});

const OS_PROGRAM_HASH: Felt =
    Felt::from_hex_unchecked("0x7E0B89C77D0003C05511B9F0E1416F1328C2132E41E056B2EF3BC950135360F");
const MULTI_BLOCK0_HASH: Felt =
    Felt::from_hex_unchecked("0x1C5CA4BCC4C03D843B8C08F9C8628BA7A108D2B62F4C0F6EF224F250679230E");
const MULTI_BLOCK1_HASH: Felt =
    Felt::from_hex_unchecked("0x378294C261592B32272381910BCB2402A864E1CDF68EDC855CAA24CACF68B65");
const ROOT0: Felt = Felt::ZERO;
const ROOT1: Felt =
    Felt::from_hex_unchecked("0x3BCBB6FD22F39E772ACE7F905AC64FBF6D7139CAC2C44189D59B37618BB62D0");
const ROOT2: Felt =
    Felt::from_hex_unchecked("0x269DDFB6E729A030E3513A7E8208D68BE9AB97852681FB531E7FC69FAC2852A");

const CONTRACT_ADDR0: Felt =
    Felt::from_hex_unchecked("0x2E9D5D85CEA6989999E86023CAD0B578825667C4DB413F3DAC8B4569A209F01");
const CONTRACT_ADDR1: Felt =
    Felt::from_hex_unchecked("0x3BBF5259540526B676273C9BE35F79DA62B07F0EDD0FD3E80F8BD1CE9F4A460");
const CONTRACT_ADDR2: Felt =
    Felt::from_hex_unchecked("0x42593E24F58291B1D7E4FD081AE6DD88D0B198E23C3F722E7E5A7A4C7BCD3D5");
const CLASS_HASH0_0: Felt =
    Felt::from_hex_unchecked("0x178286A1179F01D8A55F34B8CC651C7DD7B298B222A392197E703C3F8E161DE");
const CLASS_HASH0_1: Felt =
    Felt::from_hex_unchecked("0x39AB5549FE5E57DA8C8581AE51A0E42D9A15296BFF9BD3D7513A769CF20F7E3");
const CLASS_HASH1_0: Felt =
    Felt::from_hex_unchecked("0x55791A41352DE2EDC137AF1A2C68B9037267538FCA5119749E76430023CB01A");
const CLASS_HASH1_1: Felt =
    Felt::from_hex_unchecked("0x3676FAA37D4816933AC54BD1D90E230DB0BBB43F108CDF51555584D69A43A82");
const CLASS_HASH2_0: Felt =
    Felt::from_hex_unchecked("0x6D2819C30302763858FEC692B69ED9C9B51C4B0973F8EF8B947FF08F3D671BD");
const STORAGE_KEY0: Felt =
    Felt::from_hex_unchecked("0x0B6CE5410FCA59D078EE9B2A4371A9D684C530D697C64FBEF0AE6D5E8F0AC72");
const STORAGE_KEY1: Felt =
    Felt::from_hex_unchecked("0x110E2F729C9C2B988559994A3DACCD838CF52FAF88E18101373E67DD061455A");
const STORAGE_KEY2: Felt =
    Felt::from_hex_unchecked("0x1390569BB0A3A722EB4228E8700301347DA081211D5C2DED2DB22EF389551AB");
const STORAGE_KEY3: Felt =
    Felt::from_hex_unchecked("0x1024A17A64F318C191BAB4FEEEDA0A65B420FF92861FFB021759F05A2598ABF");
const STORAGE_KEY4: Felt =
    Felt::from_hex_unchecked("0x7C53010B8E69908E662971B823582B951E5B8E85A557C4BD3B0666428C3E520");

const STORAGE_VALUE0_0: Felt =
    Felt::from_hex_unchecked("0x346C2C2E73F8E0D5C1F3C9E2DB1CCA9B2315AD0857C4F26B076554BA4095558");
const STORAGE_VALUE0_1: Felt =
    Felt::from_hex_unchecked("0x15A44BFBB65C4961F54BC84CADBFC542AA8529E293E9FD7D45E3008DD75F376");
const STORAGE_VALUE0_2: Felt =
    Felt::from_hex_unchecked("0x31F90D664D5604B8B38C9C442B005B7E41BDA662E6E15A7364220D633153F35");
const STORAGE_VALUE1_0: Felt =
    Felt::from_hex_unchecked("0x141BF4A595FFC14E970EA6BE186A9462E20DCFBD7782E03AAEF08E9539B82D1");
const STORAGE_VALUE1_1: Felt =
    Felt::from_hex_unchecked("0x2456E7A60B3AB8B28E9AB0D9FBF0D437CCDDC9776664AF33FFD6506FC1AB8E1");
const STORAGE_VALUE2_0: Felt = Felt::ZERO;
const STORAGE_VALUE2_1: Felt =
    Felt::from_hex_unchecked("0x20E9DCD4DDB159970BD2D51075C8CC823E68BB04777FABB65879E0EA455AEE1");
const STORAGE_VALUE3_0: Felt =
    Felt::from_hex_unchecked("0x246444B2DD74265D4273FF3E41356D82B9E9A40212AE11A33C6A5EEDD2963A4");
const STORAGE_VALUE3_2: Felt =
    Felt::from_hex_unchecked("0x3C87090C322CC7E56F05DA6AEE18B28F8DD98A787F5280BD9469B00E08AFC43");
const STORAGE_VALUE4_0: Felt =
    Felt::from_hex_unchecked("0x34F56302DB42AD3B7BEC08E6CB3F786684A20143BD623419F23FDCFD29FC1D1");
const STORAGE_VALUE4_2: Felt =
    Felt::from_hex_unchecked("0x6159FC48B5236D772E81DDB785BB9FC60D97308AFFB21FEFAD8E90DDF280BC2");

const COMPILED_CLASS_HASH0_0: Felt =
    Felt::from_hex_unchecked("0x2E2D36CD2DEFEC6CF7E095CB3186F8C5025233DC7A12B26A9EBEDBC1ACC15FD");
const COMPILED_CLASS_HASH0_1: Felt =
    Felt::from_hex_unchecked("0x1B934F1068AF398C685BF2D5A9083F7817485F2356DDF6CECF25C8085DADA96");
const COMPILED_CLASS_HASH0_2: Felt =
    Felt::from_hex_unchecked("0x207385E0C41F9BF8E0616781859A6D203CEC08B4C0CBB7087C3D8FBE8BBCC2F");
const COMPILED_CLASS_HASH1_0: Felt =
    Felt::from_hex_unchecked("0x171ADCCA37ECFD43E362AA7F5EBF94AD81A38043B946D5BEDAFAB4021567B61");
const COMPILED_CLASS_HASH1_1: Felt =
    Felt::from_hex_unchecked("0x5074A78E83098D3EC8B08A2965B1C98681AF16DF9DA04A8015DBD8BAFA8C939");

const MSG_TO_L1_0: [Felt; 5] = [
    Felt::from_hex_unchecked("0x3F9A3CD755E1C8D50080AE5C76CACB1C6CACDCDF1C467C9F0A0ABDB684A6E3D"),
    Felt::from_hex_unchecked("0x3795FD47F065CF5541F0EA7D9702450F09898EF7"),
    Felt::TWO,
    Felt::from_hex_unchecked("0xC"),
    Felt::from_hex_unchecked("0x22"),
];
const MSG_TO_L1_1: [Felt; 3] = [
    Felt::from_hex_unchecked("0x3F9A3CD755E1C8D50080AE5C76CACB1C6CACDCDF1C467C9F0A0ABDB684A6E3D"),
    Felt::from_hex_unchecked("0x3795FD47F065CF5541F0EA7D9702450F09898EF7"),
    Felt::ZERO,
];

const MSG_TO_L2_0: [Felt; 6] = [
    Felt::from_hex_unchecked("0x3795FD47F065CF5541F0EA7D9702450F09898EF7"), // FromAddress.
    Felt::from_hex_unchecked("0x3F9A3CD755E1C8D50080AE5C76CACB1C6CACDCDF1C467C9F0A0ABDB684A6E3D"), /* ToAddress. */
    Felt::TWO,                          // Nonce.
    Felt::ZERO,                         // Selector.
    Felt::ONE,                          // PayloadSize.
    Felt::from_hex_unchecked("0x1234"), // Payload.
];
const MSG_TO_L2_1: [Felt; 5] = [
    Felt::from_hex_unchecked("0x3795FD47F065CF5541F0EA7D9702450F09898EF7"), // FromAddress.
    Felt::from_hex_unchecked("0x3F9A3CD755E1C8D50080AE5C76CACB1C6CACDCDF1C467C9F0A0ABDB684A6E3D"), /* ToAddress. */
    Felt::ZERO,                         // Nonce.
    Felt::from_hex_unchecked("0x4321"), // Selector.
    Felt::ZERO,                         // PayloadSize.
];

const NUMBER_OF_BLOCKS_IN_MULTI_BLOCK: usize = 13;

#[derive(PartialEq)]
enum FailureModifier {
    BlockHash,
    BlockNumber,
    CompiledClassHash,
    None,
    OsConfigHash,
    ProgramHash,
    Root,
    StorageValue,
}

impl FailureModifier {
    /// If the input modifier is equal to self, returns an output different from x. Otherwise,
    /// returns x.
    fn corrupt_for(&self, x: Felt, modifier: Self) -> Felt {
        if self == &modifier { x + Felt::from(10u8) } else { x }
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct FactTopology {
    pub(crate) tree_structure: Vec<usize>,
    pub(crate) page_sizes: Vec<usize>,
}

impl FactTopology {
    pub(crate) fn from_output_additional_data(
        output_size: usize,
        data: &OutputBuiltinAdditionalData,
    ) -> Self {
        let tree_structure = match data.attributes.get(OUTPUT_ATTRIBUTE_FACT_TOPOLOGY).cloned() {
            Some(tree_structure) => {
                let bound = 1usize << 30;
                assert_eq!(tree_structure.len() % 2, 0, "Tree structure should be of even length.");
                assert!(!tree_structure.is_empty());
                assert!(tree_structure.len() <= 10);
                assert!(tree_structure.iter().all(|x| *x <= bound));
                tree_structure
            }
            None => {
                assert!(
                    data.pages.is_empty(),
                    "Additional pages cannot be used since the '{OUTPUT_ATTRIBUTE_FACT_TOPOLOGY}' \
                     attribute is not specified."
                );
                vec![1, 0]
            }
        };
        Self {
            tree_structure,
            page_sizes: Self::get_page_sizes_from_page_dict(output_size, &data.pages),
        }
    }

    pub(crate) fn trivial(page0_size: usize) -> Self {
        assert!(
            page0_size <= MAX_PAGE_SIZE,
            "Page size {page0_size} exceeded the maximum {MAX_PAGE_SIZE}."
        );
        Self { tree_structure: vec![1, 0], page_sizes: vec![page0_size] }
    }

    /// Returns the sizes of the program output pages, given the pages dictionary that appears in
    /// the additional attributes of the output builtin.
    fn get_page_sizes_from_page_dict(
        output_size: usize,
        pages: &HashMap<usize, PublicMemoryPage>,
    ) -> Vec<usize> {
        // Make sure the pages are adjacent to each other.

        // The first page id is expected to be 1.
        let mut expected_page_id = 1;
        // We don't expect anything on its start value.
        let mut expected_page_start = None;
        // The size of page 0 is output_size if there are no other pages, or the start of page 1
        // otherwise.
        let mut page0_size = output_size;

        for (page_id, page_start, page_size) in
            pages.iter().map(|(page_id, page)| (*page_id, page.start, page.size)).sorted()
        {
            assert_eq!(
                page_id, expected_page_id,
                "Expected page id {expected_page_id}, found {page_id}."
            );
            if page_id == 1 {
                assert!(page_start > 0, "Page start must be greater than 0.");
                assert!(
                    page_start <= output_size,
                    "Page start must be less than or equal to output size. Found {page_start}, \
                     output size is {output_size}."
                );
                page0_size = page_start;
            } else {
                assert_eq!(page_start, expected_page_start.unwrap());
            }

            assert!(page_size > 0, "Page size must be greater than 0.");
            assert!(
                page_size <= output_size,
                "Page size must be less than or equal to output size. Found {page_size}, output \
                 size is {output_size}."
            );
            expected_page_start = Some(page_start + page_size);
            expected_page_id += 1;
        }

        if !pages.is_empty() {
            assert_eq!(
                expected_page_start.unwrap(),
                output_size,
                "Pages must cover the entire program output. Expected size of \
                 {expected_page_start:?}, found {output_size}."
            );
        }

        [vec![page0_size], pages.values().map(|page| page.size).collect::<Vec<usize>>()].concat()
    }
}

fn multi_block0_output(full_output: bool) -> Vec<Felt> {
    let partial_res = [
        vec![
            // initial_root.
            ROOT0,
            // final_root.
            ROOT1,
            // Previous block number.
            Felt::ZERO,
            // New block_number.
            NUMBER_OF_BLOCKS_IN_MULTI_BLOCK.into(),
            // Previous block hash.
            Felt::ZERO,
            // New block hash.
            MULTI_BLOCK0_HASH,
            // OS program hash.
            Felt::ZERO,
            *OS_CONFIG_HASH,
            // use_kzg_da.
            Felt::ZERO,
            // full_output.
            Felt::from(full_output),
            // Messages to L1.
            MSG_TO_L1_0.len().into(),
        ],
        MSG_TO_L1_0.to_vec(),
        // Messages to L2.
        vec![MSG_TO_L2_0.len().into()],
        MSG_TO_L2_0.to_vec(),
    ]
    .concat();
    let da = [
        // Number of contracts.
        vec![Felt::TWO],
        // Contract updates.
        FullContractChanges {
            addr: ContractAddress(CONTRACT_ADDR0.try_into().unwrap()),
            prev_nonce: Nonce(Felt::ZERO),
            new_nonce: Nonce(Felt::ONE),
            prev_class_hash: ClassHash(CLASS_HASH0_0),
            new_class_hash: ClassHash(CLASS_HASH0_1),
            storage_changes: vec![
                FullContractStorageUpdate {
                    key: StorageKey(STORAGE_KEY0.try_into().unwrap()),
                    prev_value: STORAGE_VALUE0_0,
                    new_value: STORAGE_VALUE0_1,
                },
                // These keys are not sorted on purpose (STORAGE_KEY2 > STORAGE_KEY1) - to test
                // that the aggregator loads them in the given order.
                FullContractStorageUpdate {
                    key: StorageKey(STORAGE_KEY2.try_into().unwrap()),
                    prev_value: STORAGE_VALUE2_0,
                    new_value: STORAGE_VALUE2_1,
                },
                FullContractStorageUpdate {
                    key: StorageKey(STORAGE_KEY1.try_into().unwrap()),
                    prev_value: STORAGE_VALUE1_0,
                    new_value: STORAGE_VALUE1_1,
                },
            ],
        }
        .encode(full_output),
        FullContractChanges {
            addr: ContractAddress(CONTRACT_ADDR1.try_into().unwrap()),
            prev_nonce: Nonce(Felt::from(10u8)),
            new_nonce: Nonce(Felt::from(10u8)),
            prev_class_hash: ClassHash(CLASS_HASH1_0),
            new_class_hash: ClassHash(CLASS_HASH1_1),
            storage_changes: vec![FullContractStorageUpdate {
                key: StorageKey(STORAGE_KEY0.try_into().unwrap()),
                prev_value: STORAGE_VALUE0_0,
                new_value: STORAGE_VALUE0_1,
            }],
        }
        .encode(full_output),
        vec![
            // Number of classes.
            2.into(),
            // Class updates.
            CLASS_HASH0_0,
        ],
        if full_output { vec![COMPILED_CLASS_HASH0_0] } else { vec![] },
        vec![COMPILED_CLASS_HASH0_1, CLASS_HASH1_0],
        if full_output { vec![COMPILED_CLASS_HASH1_0] } else { vec![] },
        vec![COMPILED_CLASS_HASH1_1],
    ]
    .concat();

    [partial_res, if full_output { da } else { compress(&da) }].concat()
}

fn multi_block1_output(full_output: bool, modifier: FailureModifier) -> Vec<Felt> {
    let partial_res = [
        vec![
            // initial_root.
            modifier.corrupt_for(ROOT1, FailureModifier::Root),
            // final_root.
            ROOT2,
            // Previous block number.
            modifier
                .corrupt_for(NUMBER_OF_BLOCKS_IN_MULTI_BLOCK.into(), FailureModifier::BlockNumber),
            // New block number.
            Felt::from(NUMBER_OF_BLOCKS_IN_MULTI_BLOCK) + Felt::ONE,
            // Previous block hash.
            modifier.corrupt_for(MULTI_BLOCK0_HASH, FailureModifier::BlockHash),
            // New block hash.
            MULTI_BLOCK1_HASH,
            // OS program hash.
            modifier.corrupt_for(Felt::ZERO, FailureModifier::ProgramHash),
            modifier.corrupt_for(*OS_CONFIG_HASH, FailureModifier::OsConfigHash),
            // use_kzg_da.
            Felt::from(false),
            // full_output.
            Felt::from(full_output),
            // Messages to L1.
            MSG_TO_L1_1.len().into(),
        ],
        MSG_TO_L1_1.to_vec(),
        // Messages to L2.
        vec![MSG_TO_L2_1.len().into()],
        MSG_TO_L2_1.to_vec(),
    ]
    .concat();
    let da = [
        // Number of contracts.
        vec![Felt::THREE],
        // Contract updates.
        FullContractChanges {
            addr: ContractAddress(CONTRACT_ADDR0.try_into().unwrap()),
            prev_nonce: Nonce(Felt::ONE),
            new_nonce: Nonce(Felt::TWO),
            prev_class_hash: ClassHash(CLASS_HASH0_1),
            new_class_hash: ClassHash(CLASS_HASH0_1),
            storage_changes: vec![
                FullContractStorageUpdate {
                    key: StorageKey(STORAGE_KEY0.try_into().unwrap()),
                    prev_value: modifier
                        .corrupt_for(STORAGE_VALUE0_1, FailureModifier::StorageValue),
                    new_value: STORAGE_VALUE0_2,
                },
                FullContractStorageUpdate {
                    key: StorageKey(STORAGE_KEY3.try_into().unwrap()),
                    prev_value: STORAGE_VALUE3_0,
                    new_value: STORAGE_VALUE3_2,
                },
            ],
        }
        .encode(full_output),
        // Contract whose block0 changes are fully reverted by block1.
        FullContractChanges {
            addr: ContractAddress(CONTRACT_ADDR1.try_into().unwrap()),
            prev_nonce: Nonce(Felt::from(10u8)),
            new_nonce: Nonce(Felt::from(10u8)),
            prev_class_hash: ClassHash(CLASS_HASH1_1),
            new_class_hash: ClassHash(CLASS_HASH1_0),
            storage_changes: vec![FullContractStorageUpdate {
                key: StorageKey(STORAGE_KEY0.try_into().unwrap()),
                prev_value: STORAGE_VALUE0_1,
                new_value: STORAGE_VALUE0_0,
            }],
        }
        .encode(full_output),
        // Contract that only appears in this block (block1).
        FullContractChanges {
            addr: ContractAddress(CONTRACT_ADDR2.try_into().unwrap()),
            prev_nonce: Nonce(Felt::from(7u8)),
            new_nonce: Nonce(Felt::from(8u8)),
            prev_class_hash: ClassHash(CLASS_HASH2_0),
            new_class_hash: ClassHash(CLASS_HASH2_0),
            storage_changes: vec![FullContractStorageUpdate {
                key: StorageKey(STORAGE_KEY4.try_into().unwrap()),
                prev_value: STORAGE_VALUE4_0,
                new_value: STORAGE_VALUE4_2,
            }]
            .into_iter()
            // Write 256 values to test contract header packing with a large number of updates.
            .chain((0..Felt::from(N_UPDATES_SMALL_PACKING_BOUND).to_u32().unwrap()).map(|i| {
                FullContractStorageUpdate {
                    key: StorageKey((STORAGE_KEY4 + Felt::ONE + Felt::from(i)).try_into().unwrap()),
                    prev_value: Felt::ZERO,
                    new_value: Felt::ONE,
                }
            }))
            .collect(),
        }
        .encode(full_output),
        vec![
            // Number of classes.
            Felt::ONE,
            // Class updates.
            CLASS_HASH0_0,
        ],
        if full_output {
            vec![modifier.corrupt_for(COMPILED_CLASS_HASH0_1, FailureModifier::CompiledClassHash)]
        } else {
            vec![]
        },
        vec![COMPILED_CLASS_HASH0_2],
    ]
    .concat();

    [partial_res, if full_output { da } else { compress(&da) }].concat()
}

fn combined_output_da(full_output: bool) -> Vec<Felt> {
    let res = [
        vec![
            // Number of contracts.
            Felt::TWO,
        ],
        // Contract updates.
        FullContractChanges {
            addr: ContractAddress(CONTRACT_ADDR0.try_into().unwrap()),
            prev_nonce: Nonce(Felt::ZERO),
            new_nonce: Nonce(Felt::TWO),
            prev_class_hash: ClassHash(CLASS_HASH0_0),
            new_class_hash: ClassHash(CLASS_HASH0_1),
            storage_changes: vec![
                FullContractStorageUpdate {
                    key: StorageKey(STORAGE_KEY0.try_into().unwrap()),
                    prev_value: STORAGE_VALUE0_0,
                    new_value: STORAGE_VALUE0_2,
                },
                FullContractStorageUpdate {
                    key: StorageKey(STORAGE_KEY3.try_into().unwrap()),
                    prev_value: STORAGE_VALUE3_0,
                    new_value: STORAGE_VALUE3_2,
                },
                FullContractStorageUpdate {
                    key: StorageKey(STORAGE_KEY1.try_into().unwrap()),
                    prev_value: STORAGE_VALUE1_0,
                    new_value: STORAGE_VALUE1_1,
                },
                FullContractStorageUpdate {
                    key: StorageKey(STORAGE_KEY2.try_into().unwrap()),
                    prev_value: STORAGE_VALUE2_0,
                    new_value: STORAGE_VALUE2_1,
                },
            ],
        }
        .encode(full_output),
        // Note: CONTRACT_ADDR1 does not appear here since it has no diff after the squash.
        FullContractChanges {
            addr: ContractAddress(CONTRACT_ADDR2.try_into().unwrap()),
            prev_nonce: Nonce(Felt::from(7u8)),
            new_nonce: Nonce(Felt::from(8u8)),
            prev_class_hash: ClassHash(CLASS_HASH2_0),
            new_class_hash: ClassHash(CLASS_HASH2_0),
            storage_changes: vec![
                FullContractStorageUpdate {
                    key: StorageKey(STORAGE_KEY4.try_into().unwrap()),
                    prev_value: STORAGE_VALUE4_0,
                    new_value: STORAGE_VALUE4_2,
                },
            ].into_iter()
            // Write 256 values to test contract header packing with a large number of updates.
            .chain((0..Felt::from(N_UPDATES_SMALL_PACKING_BOUND).to_u32().unwrap()).map(|i| {
                FullContractStorageUpdate {
                    key: StorageKey((STORAGE_KEY4 + Felt::ONE + Felt::from(i)).try_into().unwrap()),
                    prev_value: Felt::ZERO,
                    new_value: Felt::ONE,
                }
            }))
            .collect(),
        }
        .encode(full_output),
        vec![
            // Number of classes.
            Felt::TWO,
            // Class updates.
            CLASS_HASH0_0,
        ],
        if full_output { vec![COMPILED_CLASS_HASH0_0] } else { vec![] },
        vec![COMPILED_CLASS_HASH0_2, CLASS_HASH1_0],
        if full_output { vec![COMPILED_CLASS_HASH1_0] } else { vec![] },
        vec![COMPILED_CLASS_HASH1_1],
    ]
    .concat();
    if full_output { res } else { compress(&res) }
}

fn combined_kzg_info(da: &[Felt]) -> Vec<Felt> {
    let n_blobs = 1;
    let (x, y) = polynomial_coefficients_to_kzg_commitment(
        da.iter().map(|f| Fr::from(f.to_biguint())).collect(),
    )
    .unwrap();
    let z = Poseidon::hash(&Poseidon::hash_array(da), &Poseidon::hash_array(&[x, y])).to_biguint();
    let evaluation = da
        .iter()
        .enumerate()
        .map(|(i, x)| z.modpow(&BigUint::from(i), &BLS_PRIME) * x.to_biguint())
        .sum::<BigUint>()
        % &*BLS_PRIME;
    let (evaluation_high, evaluation_low) = evaluation.div_rem(&BigUint::from(2u8).pow(128u32));
    vec![z.into(), n_blobs.into(), x, y, evaluation_low.into(), evaluation_high.into()]
}

fn combined_output(full_output: bool, use_kzg_da: bool) -> Vec<Felt> {
    let da = combined_output_da(full_output);
    vec![
        vec![
            // initial_root.
            ROOT0,
            // final_root.
            ROOT2,
            // Previous block number.
            Felt::ZERO,
            // New block_number.
            Felt::from(NUMBER_OF_BLOCKS_IN_MULTI_BLOCK + 1),
            // Previous block hash.
            Felt::ZERO,
            // New block hash.
            MULTI_BLOCK1_HASH,
            OS_PROGRAM_HASH,
            *OS_CONFIG_HASH,
            // use_kzg_da.
            Felt::from(use_kzg_da),
            // full_output.
            Felt::from(full_output),
        ],
        // KZG info.
        if use_kzg_da { combined_kzg_info(&da) } else { vec![] },
        // Messages to L1.
        vec![Felt::from(MSG_TO_L1_0.len() + MSG_TO_L1_1.len())],
        MSG_TO_L1_0.to_vec(),
        MSG_TO_L1_1.to_vec(),
        // Messages to L2.
        vec![Felt::from(MSG_TO_L2_0.len() + MSG_TO_L2_1.len())],
        MSG_TO_L2_0.to_vec(),
        MSG_TO_L2_1.to_vec(),
        if use_kzg_da { vec![] } else { da },
    ]
    .concat()
}

fn bootloader_output(full_output: bool, modifier: FailureModifier) -> Vec<Felt> {
    let block0 = multi_block0_output(full_output);
    let block1 = multi_block1_output(full_output, modifier);
    [
        vec![
            // Number of multi-blocks.
            Felt::TWO,
            Felt::from(block0.len()) + Felt::TWO,
            OS_PROGRAM_HASH,
        ],
        block0,
        vec![Felt::from(block1.len()) + Felt::TWO, OS_PROGRAM_HASH],
        block1,
    ]
    .concat()
}

#[rstest]
#[case(false, false, FailureModifier::None, None)]
#[case(true, false, FailureModifier::None, None)]
#[case(false, true, FailureModifier::None, None)]
#[case(true, true, FailureModifier::None, None)]
#[case(
    true,
    false,
    FailureModifier::BlockHash,
    Some(format!("{MULTI_BLOCK0_HASH} != {}", MULTI_BLOCK0_HASH + 10))
)]
#[case(
    true,
    false,
    FailureModifier::BlockNumber,
    Some(format!("{NUMBER_OF_BLOCKS_IN_MULTI_BLOCK} != {}", NUMBER_OF_BLOCKS_IN_MULTI_BLOCK + 10))
)]
#[case(true, false, FailureModifier::ProgramHash, Some("0 != 10".to_string()))]
#[case(
    true,
    false,
    FailureModifier::OsConfigHash,
    Some(format!("{} != {}", *OS_CONFIG_HASH, *OS_CONFIG_HASH + 10))
)]
#[case(
    true,
    false,
    FailureModifier::StorageValue,
    Some(format!("{STORAGE_VALUE0_1} != {}", STORAGE_VALUE0_1 + 10))
)]
#[case(
    true,
    false,
    FailureModifier::CompiledClassHash,
    Some(format!("{COMPILED_CLASS_HASH0_1} != {}", COMPILED_CLASS_HASH0_1 + 10))
)]
fn test_aggregator(
    #[case] full_output: bool,
    #[case] use_kzg_da: bool,
    #[case] modifier: FailureModifier,
    #[case] error_message: Option<String>,
) {
    let temp_file = NamedTempFile::new().unwrap();
    let temp_file_path = temp_file.path();

    let bootloader_output_data = bootloader_output(true, modifier);
    let aggregator_input = AggregatorInput {
        bootloader_output: Some(bootloader_output_data.clone()),
        full_output,
        da: if use_kzg_da {
            DataAvailability::Blob(temp_file_path.to_path_buf())
        } else {
            DataAvailability::CallData
        },
        debug_mode: false,
        fee_token_address: Felt::ZERO,
        chain_id: Felt::ZERO,
        public_keys: None,
    };

    // Create the aggregator hint processor.
    let mut aggregator_hint_processor =
        AggregatorHintProcessor::new(&AGGREGATOR_PROGRAM, aggregator_input);

    let result =
        run_program(LayoutName::all_cairo, &AGGREGATOR_PROGRAM, &mut aggregator_hint_processor);

    let RunnerReturnObject { raw_output, cairo_pie, mut cairo_runner } = match result {
        Err(error) => {
            assert!(error.to_string().contains(error_message.unwrap().as_str()));
            return;
        }
        Ok(runner_output) => {
            assert!(error_message.is_none());
            runner_output
        }
    };

    validate_builtins(&mut cairo_runner);

    let combined_output = combined_output(full_output, use_kzg_da);
    assert_eq!(
        raw_output.iter().collect::<Vec<&Felt>>(),
        bootloader_output_data.iter().chain(combined_output.iter()).collect::<Vec<_>>()
    );

    let BuiltinAdditionalData::Output(output_builtin_data) =
        cairo_pie.additional_data.0.get(&BuiltinName::output).unwrap()
    else {
        panic!("Output builtin data should be present in the CairoPie.");
    };
    let fact_topology =
        FactTopology::from_output_additional_data(raw_output.len(), output_builtin_data);

    if use_kzg_da {
        assert_eq!(fact_topology, FactTopology::trivial(raw_output.len()));
    } else {
        let da_len = combined_output_da(full_output).len();
        let len_without_da = raw_output.len() - da_len;
        assert_eq!(
            fact_topology,
            FactTopology {
                tree_structure: vec![2, 1, 0, 2],
                page_sizes: vec![len_without_da, da_len]
            }
        );
    }
}
