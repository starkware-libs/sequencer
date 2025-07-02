use std::fmt::{Display, Write};

/// A simple positional template with `{}` placeholders.
pub struct Template(pub &'static str);

impl Template {
    /// Renders the template by substituting `{}` placeholders with the provided args. Panics if the
    /// number of `{}` in the template doesn't match the number of args provided.
    pub fn format(&self, args: &[&dyn Display]) -> String {
        // Count how many `{}` placeholders are in the template string, and ensure the number of
        // args matches the number of placeholders.
        let placeholder_count = self.0.matches("{}").count();
        assert_eq!(
            placeholder_count,
            args.len(),
            "Template {} expects {} placeholders, but got {} args",
            self.0,
            placeholder_count,
            args.len()
        );

        // Allocate the output buffer once, with some extra capacity for each argument. This avoids
        // reallocations as we append to the string. In case of insufficient capacity, the string
        // will indeed reallocate, but this is a trade-off for performance in the common case.
        const SIZE_PER_ARG: usize = 16; // Estimated size for each argument, for initial allocation.
        let mut out = String::with_capacity(self.0.len() + SIZE_PER_ARG * args.len());

        // Walk through the template, streaming chunks + args into `out`
        let mut rest = self.0;
        for value in args {
            if let Some(i) = rest.find("{}") {
                // Write the prefix before the placeholder
                out.push_str(&rest[..i]);
                write!(out, "{value}").unwrap();
                rest = &rest[i + 2..];
            }
        }
        // Append whatever is left after the last placeholder
        out.push_str(rest);
        out
    }
}
