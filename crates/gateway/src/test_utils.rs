use starknet_types_core::felt::Felt;

use crate::compiler_version::VersionId;

pub fn create_sierra_program(version_id: &VersionId) -> Vec<Felt> {
    let version_id = version_id.0;
    vec![
        // Sierra Version ID.
        Felt::from(u64::try_from(version_id.major).unwrap()),
        Felt::from(u64::try_from(version_id.minor).unwrap()),
        Felt::from(u64::try_from(version_id.patch).unwrap()),
        // Compiler Version ID.
        Felt::from(u64::try_from(0).unwrap()),
        Felt::from(u64::try_from(0).unwrap()),
        Felt::from(u64::try_from(0).unwrap()),
    ]
}
