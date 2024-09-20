

use crate::component_definitions::{BincodeSerializable, SerdeWrapper};
use crate::trace_util::configure_tracing;


fn test_serde_native_type() {
    configure_tracing();


    let data : u32 = 8;

    let encoded = SerdeWrapper { data }    .to_bincode()    .expect("Server error serialization should succeed");
    let decoded =     SerdeWrapper::<u32>::from_bincode(&encoded).unwrap();

    assert_eq!(data, decoded.data);



}