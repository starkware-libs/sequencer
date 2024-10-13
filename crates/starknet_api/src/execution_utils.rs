use cairo_lang_runner::casm_run::format_next_item;
use starknet_types_core::felt::Felt;

pub fn format_panic_data(felts: &[Felt]) -> String {
    let mut felts = felts.iter().copied();
    let mut items = Vec::new();
    while let Some(item) = format_next_item(&mut felts) {
        items.push(item.quote_if_string());
    }
    if let [item] = &items[..] { item.clone() } else { format!("({})", items.join(", ")) }
}
