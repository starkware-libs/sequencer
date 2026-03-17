use pretty_assertions::assert_eq;
use starknet_api::deprecated_contract_class::Program;
use starknet_api::test_utils::read_json_file;

use super::{compress, decompress, decompress_from_reader, serialize_and_compress};
use crate::db::serialization::StorageSerde;

#[test]
fn bytes_compression() {
    let bytes = vec![30, 5, 23, 12, 47];
    let x = decompress(compress(bytes.as_slice()).unwrap().as_slice()).unwrap();
    assert_eq!(bytes, x);
}

#[test]
fn object_compression() {
    let program: Program = read_json_file("program.json");
    let compressed = serialize_and_compress(&program).unwrap();
    let mut buf = Vec::new();
    compressed.serialize_into(&mut buf).unwrap();
    let decompressed = decompress_from_reader(&mut buf.as_slice()).unwrap();
    let restored_program = Program::deserialize_from(&mut decompressed.as_slice()).unwrap();
    assert_eq!(program, restored_program);
}

#[test]
fn compress_decompress_reuse_correctness() {
    // Compress two different payloads back-to-back (exercises context reuse).
    let payload_a = vec![1u8; 512];
    let payload_b = vec![2u8; 1024];

    let compressed_a = compress(&payload_a).unwrap();
    let compressed_b = compress(&payload_b).unwrap();

    // Decompress in reverse order (exercises decompressor reuse with different sizes).
    let restored_b = decompress(&compressed_b).unwrap();
    let restored_a = decompress(&compressed_a).unwrap();

    assert_eq!(payload_a, restored_a);
    assert_eq!(payload_b, restored_b);
}

#[test]
fn compress_decompress_multithread_isolation() {
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
    let handles: Vec<_> = (0u8..4)
        .map(|thread_index| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let payload = vec![thread_index + 1; 512 + usize::from(thread_index) * 256];
                barrier.wait(); // Force all threads to compress concurrently.
                let compressed = compress(&payload).unwrap();
                barrier.wait(); // Force all threads to decompress concurrently.
                let restored = decompress(&compressed).unwrap();
                assert_eq!(payload, restored);
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
}
