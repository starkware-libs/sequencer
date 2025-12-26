use indoc::indoc;

pub(crate) const ENTER_SCOPE_NODE: &str = "vm_enter_scope(dict(node=node, **common_args))";

pub(crate) const ENTER_SCOPE_NEW_NODE: &str = indoc! {r#"
ids.child_bit = 0 if case == 'left' else 1
new_node = left_child if case == 'left' else right_child
vm_enter_scope(dict(node=new_node, **common_args))"#};

pub(crate) const ENTER_SCOPE_NEXT_NODE_BIT_0: &str = indoc! {r#"
new_node = left_child if ids.bit == 0 else right_child
vm_enter_scope(dict(node=new_node, **common_args))"#};

pub(crate) const ENTER_SCOPE_NEXT_NODE_BIT_1: &str = indoc! {r#"
new_node = left_child if ids.bit == 1 else right_child
vm_enter_scope(dict(node=new_node, **common_args))"#};

pub(crate) const ENTER_SCOPE_LEFT_CHILD: &str =
    "vm_enter_scope(dict(node=left_child, **common_args))";

pub(crate) const ENTER_SCOPE_RIGHT_CHILD: &str =
    "vm_enter_scope(dict(node=right_child, **common_args))";

pub(crate) const ENTER_SCOPE_DESCEND_EDGE: &str = indoc! {r#"
new_node = node
for i in range(ids.length - 1, -1, -1):
    new_node = new_node[(ids.word >> i) & 1]
vm_enter_scope(dict(node=new_node, **common_args))"#};

pub(crate) const SET_SIBLINGS: &str = "memory[ids.siblings], ids.word = descend";

pub(crate) const IS_CASE_RIGHT: &str = "memory[ap] = int(case == 'right') ^ ids.bit";

pub(crate) const SET_AP_TO_DESCEND: &str = indoc! {r#"
descend = descent_map.get((ids.height, ids.path))
memory[ap] = 0 if descend is None else 1"#};

pub(crate) const ASSERT_CASE_IS_RIGHT: &str = "assert case == 'right'";

pub(crate) const WRITE_CASE_NOT_LEFT_TO_AP: &str = "memory[ap] = int(case != 'left')";

pub(crate) const SPLIT_DESCEND: &str = "ids.length, ids.word = descend";

pub(crate) const DECODE_NODE: &str = indoc! {r#"
from starkware.python.merkle_tree import decode_node
left_child, right_child, case = decode_node(node)
memory[ap] = int(case != 'both')"#};

pub(crate) const DECODE_NODE_2: &str = indoc! {r#"
from starkware.python.merkle_tree import decode_node
left_child, right_child, case = decode_node(node)
memory[ap] = 1 if case != 'both' else 0"#};

pub(crate) const SET_BIT: &str = "ids.bit = (ids.edge.path >> ids.new_length) & 1";

pub(crate) const PREPARE_PREIMAGE_VALIDATION_NON_DETERMINISTIC_HASHES: &str = indoc! {r#"
from starkware.python.merkle_tree import decode_node
left_child, right_child, case = decode_node(node)
left_hash, right_hash = preimage[ids.node]

# Fill non deterministic hashes.
hash_ptr = ids.current_hash.address_
memory[hash_ptr + ids.HashBuiltin.x] = left_hash
memory[hash_ptr + ids.HashBuiltin.y] = right_hash

if __patricia_skip_validation_runner:
    # Skip validation of the preimage dict to speed up the VM. When this flag is set,
    # mistakes in the preimage dict will be discovered only in the prover.
    __patricia_skip_validation_runner.verified_addresses.add(
        hash_ptr + ids.HashBuiltin.result)

memory[ap] = int(case != 'both')"#};

pub(crate) const BUILD_DESCENT_MAP: &str = indoc! {r#"
from starkware.cairo.common.patricia_utils import canonic, patricia_guess_descents
from starkware.python.merkle_tree import build_update_tree

# Build modifications list.
modifications = []
DictAccess_key = ids.DictAccess.key
DictAccess_new_value = ids.DictAccess.new_value
DictAccess_SIZE = ids.DictAccess.SIZE
for i in range(ids.n_updates):
    curr_update_ptr = ids.update_ptr.address_ + i * DictAccess_SIZE
    modifications.append((
        memory[curr_update_ptr + DictAccess_key],
        memory[curr_update_ptr + DictAccess_new_value]))

node = build_update_tree(ids.height, modifications)
descent_map = patricia_guess_descents(
    ids.height, node, preimage, ids.prev_root, ids.new_root)
del modifications
__patricia_skip_validation_runner = globals().get(
    '__patricia_skip_validation_runner')

common_args = dict(
    preimage=preimage, descent_map=descent_map,
    __patricia_skip_validation_runner=__patricia_skip_validation_runner)
common_args['common_args'] = common_args"#
};

pub(crate) const LOAD_EDGE: &str = indoc! {r#"
ids.edge = segments.add()
ids.edge.length, ids.edge.path, ids.edge.bottom = preimage[ids.node]
ids.hash_ptr.result = ids.node - ids.edge.length
if __patricia_skip_validation_runner is not None:
    # Skip validation of the preimage dict to speed up the VM. When this flag is set,
    # mistakes in the preimage dict will be discovered only in the prover.
    __patricia_skip_validation_runner.verified_addresses.add(
        ids.hash_ptr + ids.HashBuiltin.result)"#
};

pub(crate) const LOAD_BOTTOM: &str = indoc! {r#"
ids.hash_ptr.x, ids.hash_ptr.y = preimage[ids.edge.bottom]
if __patricia_skip_validation_runner:
    # Skip validation of the preimage dict to speed up the VM. When this flag is
    # set, mistakes in the preimage dict will be discovered only in the prover.
    __patricia_skip_validation_runner.verified_addresses.add(
        ids.hash_ptr + ids.HashBuiltin.result)"#
};

pub(crate) const HEIGHT_IS_ZERO_OR_LEN_NODE_PREIMAGE_IS_TWO: &str =
    "memory[ap] = 1 if ids.height == 0 or len(preimage[ids.node]) == 2 else 0";
