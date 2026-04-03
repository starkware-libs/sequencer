# Code Guidelines

Reusable lessons from code reviews and debugging in this codebase. Update this when a review or debugging session reveals a generalizable pattern.

---

## Naming

### Every name must answer "what is this?"
- A variable name should be a noun or noun phrase that identifies what it holds — readable without surrounding context
- The core test: if you see the name on its own line, can you tell what it represents?

### No standalone adjectives
- Adjectives describe a quality but not the thing itself; always pair with the noun
- Bad: `pending`, `remaining`, `discovered`, `complete`, `invalid`
- Good: `pending_futures`, `remaining_channels`, `discovered_channels`, `complete_entries`, `invalid_keys`

### No single-letter variables
- Single letters carry no meaning outside of trivial closures passed to stdlib combinators
- Bad: `i`, `s`, `n`, `f` as local variables
- Good: `channel_offset`, `channel_slots`, `num_items`, `felt_value`
- Exception: Single-letter closure params where the type and context make the meaning obvious (e.g., `.map(|x| x + 1)`, `.filter(|c| c.is_complete())`)

### No contractions or abbreviations
- Spell out the full word; the saved keystrokes aren't worth the mental tax on readers
- Bad: `ch`, `sc`, `futs`, `val`
- Good: `channel`, `subchannel`, `pending_futures`, `value`
- Exception: Abbreviations established by the domain (e.g., `tx` for transaction, `addr` for address, `pk` for public key, `ctx` for context, `msg` for message)

### Disambiguate counts from collections
- When a name represents a count, make that explicit with a prefix (`n_`, `num_`) or suffix (`_count`)
- Bad: `transactions` (Vec or count?), `total` (total of what?)
- Good: `num_transactions`, `transaction_count`

### Consistent terminology
- Use the same term for the same concept across parameters, fields, and documentation
- If a term is renamed, rename it everywhere — partial migrations create confusion

---

## Documentation

### Match documentation to code identifiers
- Doc comments should use the exact parameter and field names from the code

### Document semantic meaning, not just types
- Clarify behavioral details that the type signature doesn't convey
- For indices: inclusive vs exclusive; for optionals: what `None` means; for ranges: whether bounds are included

---

## Async

### Cancellation safety
- Be careful with `select!` — understand which futures are cancel-safe
- Dropped futures may leave shared state in an inconsistent state

---

## Edge Cases

### Treat user-provided values as adversarial
- Any value deserialized from an HTTP request, query parameter, or other external input must be assumed hostile
- Trace user-controlled values through the full call graph — can they cause DoS, OOM, panics, or resource exhaustion?
- Cap allocations derived from user input with hard limits

### Never panic on data reachable from requests
- Code reachable from HTTP handlers or external input must never use `.unwrap()`, `.expect()`, or unchecked indexing on values derived from that input
- Reserve panics for compile-time invariants where failure is a programmer bug, not a runtime possibility
- Return `Result` with a descriptive error variant, or use saturating/capping alternatives

### Prefer defensive arithmetic
- Use operations that handle edge cases gracefully, even if guards exist
- Prefer `saturating_sub`, `checked_mul`, etc. over raw arithmetic that could underflow/overflow if guards are later refactored

### Prefer intuitive API semantics over internal convenience
- Design APIs so callers don't need to know implementation details
- Avoid requiring callers to do adjustments like `start_index + 1`

### Simplify when defaults add no value
- If `None` just means a default value, consider using a plain type instead
- `start_index: u64` with default 0 is simpler than `Option<u64>` where `None` means 0

---

## Brevity

### Inline expressions that save >2 lines
- Inline expressions where doing so saves more than 2 lines without making the resulting line excessively long
- Prefer `map_or(default, |x| x + 1)` over `map(|x| x + 1).unwrap_or(default)`

### Inline trivial expressions
- Avoid separate `let` bindings for trivial expressions like `.clone()` when used immediately
- Inline directly in function arguments if it doesn't hurt readability

### Check for existing utilities before adding new ones
- Before writing a local helper function, search the codebase for existing shared utilities
- If similar code exists in multiple places, extract to a shared module

---

## Comments

### Explain WHY, not WHAT
- Add comments where the reasoning isn't obvious from context
- Focus on decisions, constraints, and non-obvious requirements

### No visual separator comments
- Don't use decorative comment lines to delineate sections (e.g., `// -----------`, `// =========`)
- Rely on blank lines and module organization for readability

### Document implicit structures
- When code relies on conventions or layouts, make them explicit
- Storage layouts, protocol-specific ordering, cryptographic choices

---

## Testing

### Test files live in separate `_test.rs` files
- Never write `#[cfg(test)] mod tests { ... }` inline in the source file
- Create a sibling file named `<module>_test.rs` and link it with `#[cfg(test)] #[path = "<module>_test.rs"] mod <module>_test;` at the top of the source file
- This is the codebase convention — see any module in `crates/apollo_storage/src/` for examples

### Never mask test failures
- Failing tests indicate real problems; hiding them hides bugs
- Fix the root cause instead of using `#[ignore]` or similar
- If a test fails due to stale fixture data, regenerate the fixture — don't ignore the test

### Cover edge cases systematically
- Test boundary conditions, empty inputs, and failure modes
- For pagination, test with no items, exactly one page, and partial pages

### Use accurate test names
- Test names should precisely describe the scenario being verified
- `test_empty_collection` vs `test_no_new_items` convey different conditions

### Match assertions to fixture guarantees
- Only assert on data presence when the fixture explicitly guarantees that data exists
- For structural/protocol tests, verify correctness of whatever data is returned without assuming specific content

### Verify base state before debugging failures
- When a test fails unexpectedly, first verify the code at HEAD actually compiles and tests pass
- `git stash && cargo build` to check if the original code even compiles before investigating test logic

---

## Module Organization

### Public entry points first
- Place public functions at the top of the module, before their private helpers
- Readers should encounter the high-level orchestration first and drill into details top-down

### Imports at scope top, not inline
- Place `use` statements at the top of the scope they serve — module-level for module-wide usage, `#[cfg(test)] mod tests` top for test-only usage
- Never put `use` inside function bodies; hoist to the enclosing module
- Use short imported names in signatures and bodies, not inline qualified paths like `super::types::Foo`

---

## Lessons

<!-- Add new lessons from code reviews and debugging below. Periodically promote them to a named section above. -->
