use pretty_assertions::assert_eq;

use crate::type_name::short_type_name;

struct TestStruct {}

struct GenericTestStruct<T> {
    _placeholder: T,
}

mod submodule {
    pub struct SubmoduleStruct {}
}

#[test]
fn resolve_project_relative_path_success() {
    assert_eq!(short_type_name::<TestStruct>().as_str(), "TestStruct");
    assert_eq!(short_type_name::<GenericTestStruct<u32>>().as_str(), "GenericTestStruct<u32>");
    assert_eq!(
        short_type_name::<GenericTestStruct<submodule::SubmoduleStruct>>().as_str(),
        "GenericTestStruct<SubmoduleStruct>"
    );
}
