/// All input needed to initialise the execution helper.
// TODO(Dori): Add all fields needed to compute OsCommitments, initialise a CachedState and other
//   data required by the execution helper.
pub struct StarknetOsInput {}

/// All commitment data required for the OS.
// TODO(Dori): Add all required data for commitments: roots, patricia witnesses.
pub struct OsCommitments {}

impl OsCommitments {
    pub fn new(_os_input: &StarknetOsInput) -> Self {
        todo!()
    }
}
