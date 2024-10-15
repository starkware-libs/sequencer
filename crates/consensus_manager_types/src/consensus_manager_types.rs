use std::fmt::Debug;

use derive_more::Display;
use serde::{Deserialize, Serialize};

// TODO (Matan) decide on the id structure
#[derive(
    Copy,
    Clone,
    Debug,
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Default,
    Display,
    Hash,
)]
pub struct ProposalId(pub u64);
