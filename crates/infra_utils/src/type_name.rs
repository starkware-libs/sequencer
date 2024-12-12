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
/// The function maintains two indices, `back` and `ahead`, where:
/// - `back` tracks the start of the current segment.
/// - `ahead` iterates through the string.
///
/// The function processes the string as follows:
/// 1. If `ahead` finds two consecutive colons (`::`), it moves `back` to just after the second
///    colon, effectively skipping the module path for the current segment.
/// 2. If `ahead` finds a delimiter (`<`, `,` and '>', for handling generic parameters), it slices
///    the string from `back` to `ahead` (excluding the prefix) and appends it to the output buffer,
///    followed by the delimiter.
/// 3. After the loop, the final segment (from `back` to the end of the string) is added to the
///    buffer.
///
/// This approach works because it leverages the predictable structure of Rust type paths
/// (e.g., `module::submodule::Type`) and processes the string in a single pass.
///
/// This algorithm works because:
/// - It efficiently tracks where module paths (`::`) should be truncated.
/// - It handles generic parameters (`<`, `,`, `>`) by resetting the `back` index appropriately.
/// - It processes the string in a single pass, ensuring linear time complexity.
fn truncate_type(input: &str) -> String {
    let mut buffer = String::new();
    let mut back = 0;

    let chars: Vec<char> = input.chars().collect();
    let mut ahead = 0;

    while ahead < chars.len() {
        if chars[ahead] == ':' && ahead + 1 < chars.len() && chars[ahead + 1] == ':' {
            // Move `back` to just after the second ':'
            back = ahead + 2;
            ahead += 1; // Skip the second ':'
        } else if chars[ahead] == '<' || chars[ahead] == ',' || chars[ahead] == '>' {
            // Add the slice from `back` to `ahead` to the buffer
            if back < ahead {
                buffer.push_str(input[back..ahead].trim());
                buffer.push(chars[ahead]);
            }
            back = ahead + 1; // Move `back` just after the current delimiter
        }
        ahead += 1;
    }

    // Add the final slice from `back` to `ahead` to the buffer
    if back < ahead {
        buffer.push_str(input[back..ahead].trim());
    }

    buffer
}
