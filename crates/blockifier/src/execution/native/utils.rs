use cairo_lang_starknet_classes::contract_class::ContractEntryPoint;
use itertools::Itertools;
use starknet_api::core::EntryPointSelector;
use starknet_types_core::felt::Felt;

pub fn contract_entrypoint_to_entrypoint_selector(
    entrypoint: &ContractEntryPoint,
) -> EntryPointSelector {
    EntryPointSelector(Felt::from(&entrypoint.selector))
}

pub fn encode_str_as_felts(msg: &str) -> Vec<Felt> {
    const CHUNK_SIZE: usize = 32;

    let data = msg.as_bytes().chunks(CHUNK_SIZE - 1);
    let mut encoding = vec![Felt::default(); data.len()];
    for (i, data_chunk) in data.enumerate() {
        let mut chunk = [0_u8; CHUNK_SIZE];
        chunk[1..data_chunk.len() + 1].copy_from_slice(data_chunk);
        encoding[i] = Felt::from_bytes_be(&chunk);
    }
    encoding
}

// Todo(rodrigo): This is an opinionated way of interpretting error messages. It's ok for now but I
// think it can be improved; (for example) trying to make the output similar to a Cairo VM panic
pub fn decode_felts_as_str(encoding: &[Felt]) -> String {
    let bytes_err: Vec<_> =
        encoding.iter().flat_map(|felt| felt.to_bytes_be()[1..32].to_vec()).collect();

    match String::from_utf8(bytes_err) {
        // If the string is utf8 make sure it is not prefixed by no null chars. Null chars in
        // between can still happen
        Ok(s) => s.trim_matches('\0').to_owned(),
        // If the string is non-utf8 overall, try to decode them as utf8 chunks of it and keep the
        // original bytes for the non-utf8 chunks
        Err(_) => {
            let err_msgs = encoding
                .iter()
                .map(|felt| match String::from_utf8(felt.to_bytes_be()[1..32].to_vec()) {
                    Ok(s) => format!("{} ({})", s.trim_matches('\0'), felt),
                    Err(_) => felt.to_string(),
                })
                .join(", ");
            format!("[{}]", err_msgs)
        }
    }
}
