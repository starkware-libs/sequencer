/// The shared_execution_objects crate contains execution objects that are passed, after execution
/// of a block, from the blockifier to the python code through batcher and cende in the
/// decentralized setting and through native_blockifier in the centralized setting.
pub mod central_objects;
// TODO(Yael): add the execution objects from the blockifier: execution_info, bouncer_weights,
// commitment_state_diff.
