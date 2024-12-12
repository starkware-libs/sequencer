use std::any::type_name;

#[cfg(test)]
#[path = "type_name_test.rs"]
mod type_name_test;

pub fn short_type_name<T: ?Sized>() -> String {
    let full_name = type_name::<T>();
    truncate_type(full_name)
}

/// Truncates a fully qualified Rust type name to remove module paths, leaving only the type name
/// and generic parameters.
///
/// # Algorithm
/// The function maintains two indices, `segment_start` and `char_iter`, where:
/// - `segment_start` tracks the start of the current segment.
/// - `char_iter` iterates through the string.
///
/// The function processes the string as follows:
/// 1. If `char_iter` finds two consecutive colons (`::`), it moves `segment_start` to just after
///    the second colon, effectively skipping the module path for the current segment.
/// 2. If `char_iter` finds a delimiter (`<`, `,` and '>', for handling generic parameters), it
///    slices the string from `segment_start` to `char_iter` (excluding the prefix) and appends it
///    to the output result, followed by the delimiter.
/// 3. After the loop, the final segment (from `segment_start` to the end of the string) is added to
///    the result.
///
/// This approach works because it leverages the predictable structure of Rust type paths
/// (e.g., `module::submodule::Type`) and processes the string in a single pass.
///
/// This algorithm works because:
/// - It efficiently tracks where module paths (`::`) should be truncated.
/// - It handles generic parameters (`<`, `,`, `>`) by resetting the `segment_start` index
///   appropriately.
/// - It processes the string in a single pass, ensuring linear time complexity.

/// Truncates a fully qualified Rust type name by removing module paths, leaving only the type
/// name and its generic parameters.
///
/// # Description
/// This function processes a Rust type string containing module paths, type names, and generic
/// parameters, such as:
/// ```text
/// starknet_sequencer_infra::component_client::local_component_client::LocalComponentClient<starknet_batcher_types::communication::BatcherRequest, starknet_batcher_types::communication::BatcherResponse>
/// ```
/// It removes the module paths (`::module_name`) and keeps only the type name and its
/// generic parameters:
/// ```text
/// LocalComponentClient<BatcherRequest, BatcherResponse>
/// ```
///
/// # Algorithm
/// - Iterates over the input string using a character iterator with indices.
/// - When encountering two consecutive colons (`::`), skips the preceding module path.
/// - When encountering delimiters (`<`, `,`, `>`), slices the substring from the current segment
///   start to the current index, appends it to the result, and resets the segment start.
/// - At the end, appends the remaining segment to the result.
///
/// # Examples
/// ```rust,ignore
/// let input = "a::b::c::Type<d::e::Inner, f::g::Other>";
/// let output = truncate_type(input);
/// assert_eq!(output, "Type<Inner, Other>");
/// ```
///
/// # Panics
/// This function does not panic as it only operates on valid UTF-8 strings.
///
/// # Complexity
/// The function runs in O(n) time, where `n` is the length of the input string.
///
/// # Limitations
/// - The function assumes well-formed Rust type strings. Incorrectly formatted input may yield
///   unexpected results.
///
/// # Returns
/// A new `String` with module paths removed and generic parameters preserved.
fn truncate_type(input: &str) -> String {
    let mut result = String::new();
    let mut segment_start = 0;
    let mut iter = input.char_indices().peekable();

    while let Some((index, c)) = iter.next() {
        if c == ':' {
            if let Some((_, next_char)) = iter.peek() {
                if *next_char == ':' {
                    // Skip the next ':'
                    iter.next();
                    segment_start = index + 2; // Move `segment_start` after the second ':'
                }
            }
        } else if c == '<' || c == ',' || c == '>' {
            // Add the slice from `segment_start` to the current index to the result
            if segment_start < index {
                result.push_str(input[segment_start..index].trim());
                result.push(c);
            }
            segment_start = index + 1; // Move `segment_start` after the current delimiter
        }
    }

    // Add the final slice from `segment_start` to the end
    if segment_start < input.len() {
        result.push_str(input[segment_start..].trim());
    }

    result
}
