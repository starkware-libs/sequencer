# Rust Coding Conventions

## File Layout

### Main Principle

The prime guideline for the ordering of items anywhere (files/modules/structs/traits), is by _importance_: move things further up if you think the likely reader will be looking for them when viewing the file.

Almost all other layout guidelines derive from this guideline, and it should be the default sort order if cases not denoted below.

**Rationale**: this reduces the amount of time a reader has to spend reading the file to find what they want, and enables short-circuiting from reading the whole file.

### High-level Layout

Should be applied transitively inside inner items as much as possible (see inside `impl Foo` for an example)

```rust
// Module definitions (mod foo)

// Imports

// All typedefs (type Bar = ...)

// All consts

// Types/free-functions, order by importance, one blank line between any two items (note that rustfmt cannot enforce blank lines properly).

pub struct Foo {}
// Blank line here.
// Directly after type definition.
impl Foo {
    /// Same layout here as in the file.

    pub type X = ...
    type Y = ...

    pub const STONKOS = ...
    const NOT_STONKOS = ...

    pub fn foo() {}

    fn bar() {}
}
// Blank line here.
// Directly after impl block, if exists
impl Fooable for Foo {}

// Foo is more important than Bar, so the `impl` appears after Foo's impl block, rather than after `Bar`s.
impl From<Foo> for Bar {}

// Free-functions can appear between struct/enum definitions if they are more important than them.
pub fn important_function() {}

pub struct Bar {}

// The less public an items is, the further down it appears.
struct Baz {}

fn not_important_func() {}

#[cfg(any(test,feature=testing))]
pub fn make_foo() {}
```

### Ordering Q/A

**a. If `Foo` includes `Bar` as a field, should `Bar` appear before `Foo`?**

Depends. if `Foo` can be reasoned about without having to read `Bar`, and `Foo` is deemed more important, it's perfectly fine for it to appear arbitrarily above `Bar`. Conversely, if `Foo` cannot be understood without reading `Bar`, then `Bar` is deemed more _important_ than `Foo`.

**b. If a const/typedef is only used inside one of the items (structs/functions), should it still be at the top of the file?**

Consider pushing the const inside the impl block/function if it makes sense, otherwise keep it at the top of the file.

## Error Handling & Safety

Use `Result<T, E>` for recoverable errors and `panic!` only for bugs.
Use `unreachable!` for impossible states, `todo!` for to-be-completed code sections, and `unimplemented!` for APIs that are NOT planned on being implemented.

### Error Types

Use `thiserror` for library errors and `anyhow` for binary errors.

Whenever possible, implement `PartialEq` on error enums, otherwise it's not possible to `assert_eq` on `Result`s with that error.

**Error messages formatting**: [use _lowercase_ sentences, _without_ punctuation](https://doc.rust-lang.org/stable/std/error/trait.Error.html): nested errors typically string-interpolate the inner error.

### Arithmetic

Always _consider_ using `checked_add` or `saturating_add` (depending on usage) instead of raw arithmetic operations like `+`, ditto for other mathematical operations.
**Rationale**: On debug builds, overflows and underflows panic, but in production they saturate, which might not be the intended behavior!

### Assertions

Prefer `expect` with useful information rather than unwrap, and use `unwrap` either in tests, or in absolutely trivial, bulletproof scenarios.

In more detail: use `expect` only if you have additional information that explain why the value is assumed to exist. [Do not use `expect` as a boilerplate replacement for `unwrap`](https://doc.rust-lang.org/std/error/index.html#common-message-styles), this ends up just repeating information already encoded in rust's panic message:

```rust
// BAD
// This is equivalent to unwrap, the expect message just repeats what will already be present in the panic message.
std::env::var("IMPORTANT_PATH").expect("env variable `IMPORTANT_PATH` is not set");

// GOOD
let path = std::env::var("IMPORTANT_PATH")
    .expect("env variable `IMPORTANT_PATH` was previously set by `wrapper_script.sh`");
```

Don't run any non-const code inside `expect`, it is evaluated eagerly which can have unexpected side effects, or unnecessary string allocations. See also [Lazy Gotchas](#lazy-gotchas---foo_or-vs-foo_or_else) section.

### Return Values

Always check return values from functions/methods, especially when modifying collections, which typically return an `Option` or a `bool`.

```rust
// BAD
map.insert(foo);

// GOOD
let result = map.insert(foo);
assert!(result, "dup-check was done previously in ...")
```

### Maps/Sets

Avoid iterating `HashSet` and `HashMap`, this is not deterministic in rust. Either use `BTree{Map,Set}` or `Index{Map,Set}`, and prefer `BTree` when you can, as the `Index` version has O(n) removals (it uses a `Vec` for storage).

**Note**: there is a [clippy lint](https://rust-lang.github.io/rust-clippy/master/index.html#iter_over_hash_type) for this, we should use it after fixing the existing issues.

## APIs

Types appearing _in the API_ should strongly prefer `std` types, primitives, or types exposed by the crate for external use.

Rationale: Using 3rd-party-types (types not defined inside the crate, `std` or rust builtints) _in the API_ forces users to use the _exact_ version of the dependency, it won't do Cargo's regular trick of fetching multiple versions of the same dependency, because a single type is passed from one crate to another.

### Re-exports - For Internal Items.

Avoid re-exporting internal items of the crate.

Rationale: two import paths for a single item create needless choice.

Example: an internal re-export is typically made in `lib.rs`:

```rust
// BAD
// Filename: lib.rs
pub use crate::some::type::Foo;
```

This re-export makes both of these import paths work and point to the same Foo:

```rust
use crate::Foo;
use crate::some::type::Foo;
```

Alternative: none, avoid internal re-exports completely.

### Re-exports - For External Items

Avoid re-exporting external items in most cases.

Rationale: this is usually done to avoid exposing inner types (see [APIs](#APIs)), but it also pulls in trait-impls of the type and adds them to the crate's namespace, polluting the namespace.

Example: a re-export for an external type is:

```rust
pub use <some_3rd_party_crate>::some::type::Bar;
```

The above internalizes `Bar` as an inner type of the re-exporting crate. This by-itself can be nice-to-have sometimes due to [APIs](#APIs), however this will also internalize all `impl trait for Bar` into the current crate, which almost always isn't what we want.

### Boolean Function Parameters

Use only if the call-site is clear without having to use auxiliary variables, for example a setter for a boolean field, or a "yes-no question".

```rust
// GOOD
set_operation_success_status(true)

// BAD
create_transaction(true, false)

// BETTER BUT STILL BAD
// Every callsite has to do these two...
let is_dry_run = true;
let is_executable = false;
create_transaction(is_dry_run, is_executable)

// GOOD
TransactionBuilder::new().with_dry_run(true).with_execution_status(ExecutionStatus::Enabled);
```

**Rationale**: callsite is more readable, without having to resort to using auxiliary args.

**Rationale**: when bool parameters are used for configuration, like `foo(also_bar: bool)`, this very commonly breeds more configuration parameters `foo(also_bar: bool, but_maybe_also_baz: bool)` which almost always leads to impossible states (can baz be true without bar?) and are hard to reason about: "how does one know what the "real" set of configurations are?". Rethink the design, consider a builder, or a config object that validates consistency.

### Option Parameters

Prefer enums or [zero-sized-type](https://doc.rust-lang.org/book/ch05-01-defining-structs.html#unit-like-structs-without-any-fields), unless passing `None` as an argument is clear without having to refer to the signature.

```rust
// BAD
connect("server", None)  // What does None mean here?

// GOOD — Alternative with enums
connect("server", Logging::Disabled)

// GOOD — Alternative with zero-sized-types.
struct NoLogging;
impl Logger for NoLogging {/* no-op implementation */}
connect("server", NoLogging)
```

### Getter Names

A getter for the field `foo` should be `fn foo`, rather than `fn get_foo`

The same is also encouraged for non-field getters, except for in cases where `get*` really improves readability.

### `self` Mutability in Getters

Getter methods should always be `&self`, never `&mut self` --- use interior mutability if mutating self is necessary for maintenance like caching.

**Rationale**: aligning to user's expectation for getters, preventing borrow-checker surprises.

### Private Functions and Private Fields

Struct fields should be `pub` by default, unless changing them could break invariants, e.g. a field that contains a vector that must be kept sorted.

If a struct field is private, document the invariant next to the field, and don't create setters for the field.

Note: This rule is not strict, use discretion. For example, types that are included in a crate's API have other considerations, like making a field private so it won't be included in the crate's docs.rs entry or to prevent future breaking changes if that field is likely to change in some way.

### Avoid `mod.rs`

Use `submodule_name.rs`, as `mod.rs` [is considered legacy](https://doc.rust-lang.org/edition-guide/rust-2018/path-changes.html#no-more-modrs) since 2018.

**Rationale**: fuzzy-finding files is harder when you have a large number of files with the same name.

**Edge-case**: there are some rare use-cases where `mod.rs` is required, it's OK to use then.

## Types

### Newtypes

If newtyping as a "pass-through" wrapper (`Foo(pub Bar)`), add `Deref` and `DerefMut`.

Avoid "ip addresses" (`let bar = foo.0.0.0.0.1`): prefer `Deref` when the derivation is trivial, or switch to a struct wrapper instead of a tuple otherwise.

### Type Safety

Before using raw types like `u64` to represent a value, first grep the repo for existing types that wrap around it.
Rationale: mainly consistency, but also they might have constraints on the values.

## Performance & Allocations

### Allocations

Avoid allocations when a stack-based type will do, in particular: prefer arrays/iterators over vecs if possible, prefer `&str` or `impl ToString` over `String`.

Use `Vec::with_capacity()` when size is known to avoid reallocations.

### Laziness

Prefer returning lazy types from inner functions and delay the actual evaluation when possible, in practice this mostly mean holding off on `collect`ing until the actual usage.

**Motivation**: helps the compiler optimize better, usually means less allocations, and in some cases avoid allocation entirely if somewhere up the stack it short-circuits.

**Exception**: Early evaluation is still OK if the allocation is small and/or performed in a non-critical location, but only if it makes the code clearer.

### Lazy Gotchas - `foo_or` vs `foo_or_else`

Methods that end with `_or`, like `unwrap_or`, are typically eagerly evaluated, meaning `foo.unwrap_or(expensive_calculation_of_foo())` will always be evaluated, even if the original value is unwrapped --- use `_or_else` methods for lazy evaluations, which is typically what you want.
The same also holds for `expect`, and in general for all methods that take a non-closure parameter.

```rust
// BAD
// always panics!
map.get(key).expect(panic!("{key} should exist because of <reason>"));

// BAD
// Inefficient: this always allocates a String on the heap.
map.get(key).expect(format!("{key} should exist because of <reason>"));

// GOOD
map.get(key).unwrap_or_else(|| panic!("{key} should exist because of <reason>"))
```

## Testing

Use unit tests for short (< 1 sec) and threadsafe tests, otherwise use cargo integration tests.

A failure in a unit test should immediately point to the source of the issue, and the clearly display the error encountered. Moreover, it should be possible to debug a test even if the test writer is not available, for example:

-   add `#[track_caller]` on test-utils which include a critical assert, so that the trace will be the callsite in the test
-   use `assert_eq(result, Ok(<Value>))` over `assert!(result.is_ok())` --- the former will display the error, and the latter will simply say `expected True, got False`.
-   Avoid doing too many things in one test with a single assert at the end, unless other tests exist that cover enough parts of the test separately so that finding the source will be simple.

Put integration tests inside `tests/` at the crate root if they don't depend on features of their crate (this constraint is valid until [this cargo issue](https://github.com/rust-lang/cargo/issues/2911#issuecomment-1739880593) or [this cargo bug](https://github.com/rust-lang/cargo/issues/15151) get resolved), otherwise put them in a dedicated integration-test crate for the whole package (mostly relevant for multi-crate packages). Cargo integration test files are not parallelized, and are run in an anonymous crate without `cfg(test)`, which allows the test writer to simulate real UX.

To test binary crates, either add a `lib.rs` and call its main from `main.rs` and from the test, or use integration tests that spawn the binary as a subprocess (See `CARGO_BIN_EXE_<binary_name>`).

## Documentation Standards

Use `///` as doc-strings for all non-trivial structs, functions and methods, and place it before the definition --- these show up on docs.rs and editor tooltips.

Use `//` for all other comments.

Don't use `/* */` - style comments.

Don't use abbreviations: use `cannot` (rather than `can't`), `transaction` (rather than `tx`), `does not`, `it is`, etc.

In all comment types (including inline comments!) start comments with a capital letter and end with a dot.

### Textual Content Quality

All textual content, including code-comments, commit messages, `expect` messages (see also [Assertions](#assertions) section), should include _additional_ information not readily observable in the surrounding code. Take extra care when using AI-generated code, which tends to include a lot of trivial comment-bloat.

**Rationale**: clear code is self-documenting, repeating it is a waste of time.

```rust
// BAD
// Add foo to collection.
my_vec.push(foo)
```

## Style Conventions

### Associated Functions

All associated functions should depend on `Self`. Associated functions that don't should be free functions.

### Casting

If you have to cast a value due to an implementation detail, like a pass-through newtype, prefer `into` over `ImplementationDetail::from`.

Avoid using `as` casts unless there is no other choice --- they are banned by the linter in our repo.
**Rationale**: if `as` casts overflow or underflow, they saturate, and do so without printing or panicking --- most of the time this isn't the desired result, and can introduce silent bugs.
```rust
// BAD
let x = u128::MAX as u64; // This doesn't panic, it saturates and simply sets `x` as u64::MAX.
```

### Match Ergonomics

Don't use `ref` keyword, it's considered legacy, as all of its uses can be replaced more concisely with `&` on the RHS.

### Float

Avoid float types unless you're sure it's necessary. If you're sure it's necessary, avoid anyway and [read this](https://stackoverflow.com/questions/3730019/why-not-use-double-or-float-to-represent-currency).

**Rationale**: they have surprising results when performing arithmetic, ordering, and equality. Additionally (and consequently), they don't support casts to other types unless one uses `as` casts, which we are banning (see casting section).

### Turbofish

Mostly prefer type annotation a variable over turbofish, but it's OK to use sometimes for simple casts when a one-liner is preferred.

### Combinators

Avoid side-effects in pure combinators, like `map`, `filter` or `zip` --- use control flow (`for`/`if`/`match`) instead.
Using the non-pure `for_each` combinator is also fine but only for one-liners.

```rust
// BAD
// Even if rust allows the closure to mutate self, it's surprising to do this inside a transformer like `map`.
let new_vec = old_vec.map(|num| self.mutate_self(num))

// GOOD
let new_vec = Vec::with_capacity(old_vec.len());
for num in old_vec {
    let new_num = self.mutate_self(num);
    new_vec.push(new_num);
}
```

### If-let-else Pattern

Prefer `let-else` or `match` patterns, they are always more concise and incur less cognitive load

```rust
// BAD
if let Some(num) = foo {
    derp(num)
} else {
    not_derp()
}

// GOOD
match foo {
    Some(num) => derp(num)
    None => not_derp()
}

// GOOD for short-circuits
let Some(num) = foo else {
    return not_derp()
}
derp(num)
```

## Github Smell-Testing

If you have a oneliner you suspect is a bad practice, when applicable, try searching for it in the global search bar in `github.com`, if you don't see any production-grade crates doing it consider changing it.

## External Dependencies

Be conservative with helper crates, and especially avoid single-use crates --- external deps increase our compilation time, and increase potential of version clashes and sudden breaking changes (not all crates adhere to Semver properly).

If a crate has a viable use-case that doesn't require some heavy-dependency, feature-gate the dependency, this helps reduce compilation time.

### Unmaintained Dependencies

Recommended checklist before adding a new dependency:

-   Last commit should be less than a month ago
-   Small crates should have at least ~100 github stars, large ones should have ~1000.
-   Skim through the README for important notes, ESPECIALLY look for "no longer maintained" or "archived" notices.
-   If there are indications for the crate no longer being maintained, search for "maintained" or "dead" in the crate's issues. Or see if recent pull-requests received any attention from the maintainers.

### `lazy_static` crate

Do not use `lazy_static`.
This functionality [is part of `std` now](https://github.com/rust-lang-nursery/lazy-static.rs?tab=readme-ov-file#standard-library) and the crate is dead.
If you're working around code that uses lazy_static, remove it in favor of the `std` type and help us close in on removing the extra dependency from the project.

## Advanced Patterns

### Macros

The general rule for metaprogramming and language "power features" applies: only use macros if non-macro alternatives are not feasible (like when writing a DSL).

**Rationale**: [the more expressive the macro is, the less it can be reasoned about](https://matklad.github.io/2021/02/14/for-the-love-of-macros.html); well-structured non-macro code doesn't suffer from this.

For example: using parametrization in a test (using rstest) can make tests harder to reason about and debug in some cases, when the parametrization is very complicated and involves complex types. In those cases one should consider a free-function that performs the test, and several one-line tests that call it with different args, rather than forcing a parameterized solution.

### Async

Mostly avoid `join!`, as it doesn't short-circuit on errors in one of the tasks: use safe alternatives like `try_join!` (there are others that are also fine).

Mostly avoid `select!` unless you are certain the tasks are cancel-safe, this can lead to leaks and deadlocks: either spawn the tasks separately (`JoinHandle` is cancel-safe) or use safe alternatives like `FuturesUnordered` or `JoinSet`.

When spawning a task, you _must_ maintain the handle returned from the executor and make sure it completes successfully --- otherwise the task will _detach_ from the process.

## Commits/PRs

Commits should be [small](https://docs.google.com/presentation/d/1b4uTSMs16AlaY6cMWrvB1RyOM_JmQ5USW3LDB9MCFj8/edit?slide=id.g3337b86d7d1_0_23#slide=id.g3337b86d7d1_0_23), _self-contained_ and well-defined.

## AI-Generated Code Guidelines

Take extra care when reviewing ai-generated code for these issues:

-   Using deprecated/dead crates (!): for example `lazy_static` often appears in ai-code (see [lazy static section](#lazy_static-crate)).
-   Using "toy" crates (low github-starred, pet‑project crates not intended for production): just because a crate out there can fulfil a given set of requirements doesn't mean it's production-ready, see also [External Dependencies](#external-dependencies) section.
-   Excessive code-comment bloat: see [Textual Content Quality](#textual-content-quality) section.
-   Hygiene issues: leaks and races are common, since chatbots rarely take a step back and analyze the surrounding conditions in which a code is being run; always analyze flows affected by your change as the AI will not do so.
-   Excessive allocations/clones.

## Open issues

-   Derive ordering: rustfmt can't enforce alphabetization, and there isn't a clear standard in the community for this anyway. Alternative orderings include ordering the builtin derives first (Debug/Clone), then third-party dep's derives (Serialize), then custom derives, all in order of commonality (the more common the derive is, the more "to the left" it appears).
