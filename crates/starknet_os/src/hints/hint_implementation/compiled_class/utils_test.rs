use cairo_lang_starknet_classes::NestedIntList;
use rstest::rstest;
use starknet_api::felt;
use starknet_types_core::felt::Felt;

use super::{
    create_bytecode_segment_structure,
    BytecodeSegment,
    BytecodeSegmentInnerNode,
    BytecodeSegmentNode,
};
use crate::hints::hint_implementation::compiled_class::utils::BytecodeSegmentLeaf;

fn dummy_bytecode(bytecode_len: u32) -> Vec<Felt> {
    (0..bytecode_len).map(|x| felt!(x)).collect()
}

// Recursively compare nodes.
impl PartialEq for BytecodeSegmentNode {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (BytecodeSegmentNode::Leaf(a), BytecodeSegmentNode::Leaf(b)) => a == b,
            (BytecodeSegmentNode::InnerNode(a), BytecodeSegmentNode::InnerNode(b)) => {
                a.segments.len() == b.segments.len()
                    && a.segments.iter().zip(b.segments.iter()).all(|(a, b)| a == b)
            }
            _ => false,
        }
    }
}

#[rstest]
#[case (3, NestedIntList::Node(vec![NestedIntList::Node(vec![NestedIntList::Leaf(3)])]),
BytecodeSegmentNode::InnerNode(BytecodeSegmentInnerNode {
    segments: vec![BytecodeSegment {
        node: BytecodeSegmentNode::InnerNode(BytecodeSegmentInnerNode {
            segments: vec![BytecodeSegment {
                node: BytecodeSegmentNode::Leaf(BytecodeSegmentLeaf {
                    data: vec![felt!(0_u8), felt!(1_u8), felt!(2_u8)],
                }),
                length: 3,
            }],
        }),
        length: 3,
    }],
}))]
#[case (3, NestedIntList::Leaf(3), BytecodeSegmentNode::Leaf(BytecodeSegmentLeaf {
    data: vec![felt!(0_u8), felt!(1_u8), felt!(2_u8)],
}))]
#[case(4, NestedIntList::Node(vec![
    NestedIntList::Leaf(2),
    NestedIntList::Leaf(2),
]), BytecodeSegmentNode::InnerNode(BytecodeSegmentInnerNode {
    segments: vec![
        BytecodeSegment {
            node: BytecodeSegmentNode::Leaf(BytecodeSegmentLeaf {
                data: vec![felt!(0_u8), felt!(1_u8)],
            }),
            length: 2,
        },
        BytecodeSegment {
            node: BytecodeSegmentNode::Leaf(BytecodeSegmentLeaf {
                data: vec![felt!(2_u8), felt!(3_u8)],
            }),
            length: 2,
        },
    ],
}))]
#[case(4, NestedIntList::Node(vec![
    NestedIntList::Leaf(2),
    NestedIntList::Node(vec![NestedIntList::Leaf(2)]),
]), BytecodeSegmentNode::InnerNode(BytecodeSegmentInnerNode {
    segments: vec![
        BytecodeSegment {
            node: BytecodeSegmentNode::Leaf(BytecodeSegmentLeaf {
                data: vec![felt!(0_u8), felt!(1_u8)],
            }),
            length: 2,
        },
        BytecodeSegment {
            node: BytecodeSegmentNode::InnerNode(BytecodeSegmentInnerNode {
                segments: vec![BytecodeSegment {
                    node: BytecodeSegmentNode::Leaf(BytecodeSegmentLeaf {
                        data: vec![felt!(2_u8), felt!(3_u8)],
                    }),
                    length: 2,
                }],
            }),
            length: 2,
        },
    ],
}))]
#[case(10, NestedIntList::Node(vec![
    NestedIntList::Leaf(3),
    NestedIntList::Node(vec![
        NestedIntList::Leaf(1),
        NestedIntList::Leaf(1),
        NestedIntList::Node(vec![NestedIntList::Leaf(1)]),
    ]),
    NestedIntList::Leaf(4),
]), BytecodeSegmentNode::InnerNode(BytecodeSegmentInnerNode {
    segments: vec![
        BytecodeSegment {
            node: BytecodeSegmentNode::Leaf(BytecodeSegmentLeaf {
                data: vec![felt!(0_u8), felt!(1_u8), felt!(2_u8)],
            }),
            length: 3,
        },
        BytecodeSegment {
            node: BytecodeSegmentNode::InnerNode(BytecodeSegmentInnerNode {
                segments: vec![
                    BytecodeSegment {
                        node: BytecodeSegmentNode::Leaf(BytecodeSegmentLeaf {
                            data: vec![felt!(3_u8)],
                        }),
                        length: 1,
                    },
                    BytecodeSegment {
                        node: BytecodeSegmentNode::Leaf(BytecodeSegmentLeaf {
                            data: vec![felt!(4_u8)],
                        }),
                        length: 1,
                    },
                    BytecodeSegment {
                        node: BytecodeSegmentNode::InnerNode(BytecodeSegmentInnerNode {
                            segments: vec![BytecodeSegment {
                                node: BytecodeSegmentNode::Leaf(BytecodeSegmentLeaf {
                                    data: vec![felt!(5_u8)],
                                }),
                                length: 1,
                            }],
                        }),
                        length: 1,
                    },
                ],
            }),
            length: 3,
        },
        BytecodeSegment {
            node: BytecodeSegmentNode::Leaf(BytecodeSegmentLeaf {
                data: vec![felt!(6_u8), felt!(7_u8), felt!(8_u8), felt!(9_u8)],
            }),
            length: 4,
        },
    ],
}))]
fn create_bytecode_segment_structure_test(
    #[case] bytecode_len: u32,
    #[case] bytecode_segment_lengths: NestedIntList,
    #[case] expected_structure: BytecodeSegmentNode,
) {
    let bytecode = dummy_bytecode(bytecode_len);
    let actual_structure =
        create_bytecode_segment_structure(&bytecode, bytecode_segment_lengths).unwrap();

    assert_eq!(actual_structure, expected_structure);
}
