use starknet_api::hash::StarkFelt;

use crate::compiler_version::VersionId;

pub fn create_sierra_program(version_id: &VersionId) -> Vec<StarkFelt> {
    vec![
        StarkFelt::from(u64::try_from(version_id.major).unwrap()),
        StarkFelt::from(u64::try_from(version_id.minor).unwrap()),
        StarkFelt::from(u64::try_from(version_id.patch).unwrap()),
    ]
}
