/// Macro to generate a compile-time constant array containing all permutations
/// of multiple enums.
///
/// This macro:
/// - Accepts a list of tuples (`($name, $enum)`) where:
///   - `$name` is a string representing the key for the enum.
///   - `$enum` is an enum type that implements `strum::EnumVariantNames`.
/// - Computes **all possible permutations** of the provided enums at **compile-time**.
/// - Generates a uniquely named constant in the format `<ENUM1_ENUM2_PERMUTATIONS>`.
///
/// # Example
/// ```rust, ignore
/// #[derive(strum::EnumVariantNames)]
/// enum Color {
///     Red,
///     Green,
///     Blue,
/// }
///
/// #[derive(strum::EnumVariantNames)]
/// enum Size {
///     Small,
///     Medium,
///     Large,
/// }
///
/// generate_permutations!(
///     ("color", Color),
///     ("size", Size),
/// );
/// ```
///
/// # Output
/// ```text
/// [("color", "Red"), ("size", "Small")]
/// [("color", "Red"), ("size", "Medium")]
/// [("color", "Red"), ("size", "Large")]
/// [("color", "Green"), ("size", "Small")]
/// [("color", "Green"), ("size", "Medium")]
/// [("color", "Green"), ("size", "Large")]
/// [("color", "Blue"), ("size", "Small")]
/// [("color", "Blue"), ("size", "Medium")]
/// [("color", "Blue"), ("size", "Large")]
/// ```
#[macro_export]
macro_rules! generate_permutations {
    ($const_name:ident, $(($name:expr, $enum:ty)),* $(,)?) => {
        $crate::paste::paste! {
            // The generated constant containing all permutations.
            pub const $const_name: [[(&'static str, &'static str); {
                // Compute the number of enums being used in the permutations.
                [$($enum::VARIANTS.len()),*].len()
            }]; {
                // Compute the total number of permutations.
                let mut total_size = 1;
                $( total_size *= $enum::VARIANTS.len(); )*
                total_size
            }] = {
                /// An array holding references to the variant names of each enum.
                const ENUM_VARIANTS: [&'static [&'static str]; {
                    [$($enum::VARIANTS.len()),*].len()
                }] = [
                    $($enum::VARIANTS),*
                ];

                /// A constant representing the total number of permutations.
                const TOTAL_SIZE: usize = {
                    let mut product = 1;
                    $( product *= $enum::VARIANTS.len(); )*
                    product
                };

                /// A compile-time function to generate all permutations.
                ///
                /// # Arguments
                /// * `variants` - A reference to an array of slices, where each slice contains the variants of an enum.
                /// * `names` - A reference to an array of enum names.
                ///
                /// # Returns
                /// A 2D array where each row represents a unique combination of variant names across the provided enums.
                const fn expand<const N: usize>(
                    variants: [&'static [&'static str]; N],
                    names: [&'static str; N]
                ) -> [[(&'static str, &'static str); N]; TOTAL_SIZE] {
                    // The output array containing all possible variant name combinations.
                    let mut results: [[(&'static str, &'static str); N]; TOTAL_SIZE] =
                        [[("", ""); N]; TOTAL_SIZE];

                    let mut index = 0;
                    let mut counters = [0; N];

                    // Iterate over all possible permutations.
                    while index < TOTAL_SIZE {
                        let mut row: [(&'static str, &'static str); N] = [("", ""); N];
                        let mut i = 0;

                        // Assign the correct variant name to each position in the row.
                        while i < N {
                            row[i] = (names[i], variants[i][counters[i]]);
                            i += 1;
                        }

                        results[index] = row;
                        index += 1;

                        // Carry propagation for multi-dimensional iteration.
                        let mut carry = true;
                        let mut j = 0;

                        while j < N && carry {
                            counters[j] += 1;
                            if counters[j] < variants[j].len() {
                                carry = false;
                            } else {
                                counters[j] = 0;
                            }
                            j += 1;
                        }
                    }

                    results
                }

                // Calls `expand` to generate the final constant containing all permutations.
                expand(ENUM_VARIANTS, [$($name),*])
            };
        }
    };
}

/// A macro that converts a **fixed-size 2D array** into a **slice of references**.
///
/// This allows the array to be used in contexts where a dynamically sized slice (`&[&[(&str,
/// &str)]]`) is required instead of a statically sized array.
///
/// # Example Usage
/// ```rust, ignore
/// const INPUT: [[(&str, &str); 2]; 3] = [
///     [("Color", "Red"), ("Size", "Small")],
///     [("Color", "Blue"), ("Size", "Medium")],
///     [("Color", "Green"), ("Size", "Large")],
/// ];
///
/// convert_array!(PERMUTATION_SLICE, INPUT);
/// ```
///
/// # Expected Output:
/// ```rust, ignore
/// const PERMUTATION_SLICE : &[&[(&str, &str)]] = &[
///     [("Color", "Red"), ("Size", "Small")],
///     [("Color", "Blue"), ("Size", "Medium")],
///     [("Color", "Green"), ("Size", "Large")]
/// ]
/// ```
#[macro_export]
macro_rules! convert_array {
    ($name:ident, $input:expr) => {
        // A **slice reference** to the converted input array.
        // This allows the macro to return a dynamically sized slice
        // instead of a fixed-size array.
        pub const $name: &[&[(&str, &str)]] = {
            // A compile-time function to convert a fixed-size array into a slice of references.
            //
            // # Arguments
            // * `input` - A reference to a 2D array of string tuples.
            //
            // # Returns
            // A reference to an array of slices, where each slice represents a row in the input.
            const fn build_refs<'a, const M: usize, const N: usize>(
                input: &'a [[(&'a str, &'a str); N]; M],
            ) -> [&'a [(&'a str, &'a str)]; M] {
                // An array to hold the references to each row in the input.
                let mut refs: [&[(&str, &str)]; M] = [&input[0]; M];

                let mut i = 0;
                while i < M {
                    refs[i] = &input[i];
                    i += 1;
                }
                refs
            }

            // Returns a reference to the slice representation of the input array.
            &build_refs(&$input)
        };
    };
}

/// Macro to generate a permutation of enum variants and store them in a user-defined constant.
///
/// This macro:
/// - Generates an intermediate constant `<name>_PERMUTATIONS` of all permutations as an array.
/// - Generates a constant `<name>` as a reference slice to the aforementioned.
///
/// # Arguments
/// - `$const_name`: The base name used for both the intermediate and final constants.
/// - A list of `(LABEL_NAME, ENUM_TYPE)` pairs.
///
/// # Example Usage
/// ```rust, ignore
/// generate_permutation_labels!(
///     CUSTOM_LABELS_CONST,
///     (LABEL_NAME_TX_TYPE, RpcTransactionLabelValue),
///     (LABEL_NAME_SOURCE, SourceLabelValue),
/// );
/// ```
///
/// # Generated Constants
/// ```rust, ignore
/// pub const CUSTOM_LABELS_CONST_PERMUTATIONS: [[(&'static str, &'static str); N]; TOTAL_SIZE] = { ... };
/// pub const CUSTOM_LABELS_CONST: &[&[(&'static str, &'static str)]] = { ... };
/// ```
#[macro_export]
macro_rules! generate_permutation_labels {
    ($const_name:ident, $(($name:expr, $enum:ty)),* $(,)?) => {
        $crate::paste::paste! {
            // Define the intermediate permutations constant by calling `generate_permutations!`.
            $crate::generate_permutations!([<$const_name _PERMUTATIONS>], $(($name, $enum)),*);

            // Convert the intermediate permutations into a reference slice using the provided name.
            $crate::convert_array!($const_name, [<$const_name _PERMUTATIONS>]);
        }
    };
}

// TODO(Tsabary): make the generate_permutation_labels more robust with respect to its required
// imports.

#[cfg(test)]
#[path = "label_utils_test.rs"]
mod label_utils_test;
