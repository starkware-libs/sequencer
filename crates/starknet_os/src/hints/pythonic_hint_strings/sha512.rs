use indoc::indoc;

pub(crate) const SHA512_FINALIZE: &str = indoc! {r#"# Add dummy pairs of input and output.
from starkware.cairo.common.cairo_sha512.sha512_utils import (
    SHA512_IV,
    compute_message_schedule,
    sha2_compress_function,
)

number_of_missing_blocks = (-ids.n) % ids.BATCH_SIZE
assert 0 <= number_of_missing_blocks < 20
_sha512_input_chunk_size_felts = ids.SHA512_INPUT_CHUNK_SIZE_FELTS
assert 0 <= _sha512_input_chunk_size_felts < 100

message = [0] * _sha512_input_chunk_size_felts
w = compute_message_schedule(message)
output = sha2_compress_function(SHA512_IV, w)
padding = (message + SHA512_IV + output) * number_of_missing_blocks
segments.write_arg(ids.sha512_ptr_end, padding)"#};
