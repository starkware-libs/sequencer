pub mod block_context;
pub mod enum_generation;
pub mod error;
pub mod types;

pub enum Hints {
    BlockContextHint(block_context::BlockContextHint),
    BlockContextHintExtension(block_context::BlockContextHintExtension),
}
