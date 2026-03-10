# Rust Coding Conventions

## File Layout

### Main Principle

The prime guideline for ordering items in a file is by _importance_: move things further up if you think the likely reader will be looking for them when viewing the file.

**Rationale**: this reduces the amount of time a reader has to spend reading the file to find what they want, and enables short-circuiting from reading the whole file.

### Top-down Ordering

Module definitions and imports always come first.

After that, structure the file top-down: start with the public API, then the functions the public API calls directly, then the functions _those_ functions call, and so on. This lets a reader understand the file by reading it from top to bottom without having to jump around, and allows the reader to read only the API if they just want to use the code.

For tests, the public API is the tests themselves. Put the tests above and helper functions at the bottom of the file.

This rule is a guideline, not a strict law. If breaking it helps readability, use your discretion. One of the main reasons to do so is to preserve continuity. Some examples:
1. You may want all methods of a struct, both public and private, in the same `impl` block even if
below it there are public structs/functions/methods.
2. You may want functions with similar semantic meaning to sit together.
3. Usually, if a struct `Foo` includes `Bar` as a field, `Foo` should appear first.
However, if `Foo` is hard to understand without understanding `Bar`, you may put `Bar` first.

In these cases, splitting the file into submodules can help achieve both top-down ordering and continuity.

## Directory Layout

Single-file modules should be in a file called `foo.rs`. Modules with submodules should be in a file called `foo/mod.rs`.

`mod.rs` should not contain any code — it should only declare submodules.
If the module truly has general-purpose code that doesn't belong to a specific submodule,
place it in `foo/foo.rs`.

### Test File Placement

For a single-file module `foo.rs`, place tests in `foo_test.rs` in the same directory.

Use the `#[path]` attribute to make it a submodule of `foo`:

```rust
// In foo.rs
#[cfg(test)]
#[path = "foo_test.rs"]
mod foo_test;
```

For a module using `foo/mod.rs`, place tests in `foo/test.rs`.

Multiple test files are allowed when you have different types of tests.
They should end with a `_test` suffix.
For single-file modules, they should also start with a `foo_` prefix
(e.g. `foo_flow_test.rs`, `foo_regression_test.rs`).

## AI-Generated Code Guidelines

Take extra care when reviewing ai-generated code for these issues:

-   Using deprecated/dead crates (!): for example `lazy_static` often appears in ai-code (see [lazy static section](#lazy_static-crate)).
-   Using "toy" crates (low github-starred, pet‑project crates not intended for production): just because a crate out there can fulfil a given set of requirements doesn't mean it's production-ready, see also [External Dependencies](#external-dependencies) section.
-   Excessive code-comment bloat: see [Textual Content Quality](#textual-content-quality) section.
-   Hygiene issues: leaks and races are common, since chatbots rarely take a step back and analyze the surrounding conditions in which a code is being run; always analyze flows affected by your change as the AI will not do so.
-   Excessive allocations/clones.

## Testing

A failure in a unit test should immediately point to the source of the issue, and the clearly display the error encountered. Moreover, it should be possible to debug a test even if the test writer is not available, for example:

-   add `#[track_caller]` on test-utils which include a critical assert, so that the trace will be the callsite in the test
-   use `assert_eq(result, Ok(<Value>))` over `assert!(result.is_ok())` ---
the former will display the error, and the latter will simply say `expected True, got False`.
    -    If you're not interested in the value, or if error type doesn't implement `PartialEq`, simply call `result.unwrap()`
-   Avoid doing too many things in one test with a single assert at the end, unless other tests exist that cover enough parts of the test separately so that finding the source will be simple.

### Integration and Flow Tests
Use unit tests for short (< 1 sec) and threadsafe tests, otherwise use cargo integration tests:

Put flow tests and integration tests inside `tests/` at the crate root if they don't depend on features of their crate (this constraint is valid until [this cargo issue](https://github.com/rust-lang/cargo/issues/2911#issuecomment-1739880593) or [this cargo bug](https://github.com/rust-lang/cargo/issues/15151) get resolved), otherwise put them in a dedicated integration-test crate for the whole package (mostly relevant for multi-crate packages). Cargo integration test files are not parallelized, and are run in an anonymous crate without `cfg(test)`, which allows the test writer to simulate real UX.

### Binary Tests

To test binary crates, add a `lib.rs` and call its main from `main.rs` and from the test.

### Dependency Injection

Dependency injection allows you to test a component in isolation by mocking its dependencies. This focuses the test on a single unit of code without relying on external services or complex setup.

Use trait-based dependency injection with `#[automock]` from the `mockall` crate:

```rust
// In test builds, `automock` generates a `MockDatabase` struct that implements
// `Database` with configurable behavior for each method.
#[cfg_attr(test, mockall::automock)]
trait Database {
    fn get_user(&self, id: u64) -> Result<User, Error>;
}

// The generic parameter lets us inject either a real database or a mock.
struct UserService<D: Database> {
    db: D,
}

impl<D: Database> UserService<D> {
    fn get_user_name(&self, id: u64) -> Result<String, Error> {
        self.db.get_user(id).map(|user| user.name)
    }
}
```

In your test file:

```rust
use mockall::predicate::eq;

#[test]
fn test_get_user_name() {
    let mut mock_db = MockDatabase::new();
    // Configure the mock: It will allow exactly one call to `get_user` with id=1, returning a fake user.
    mock_db
        .expect_get_user()
        .with(eq(1))
        .return_once(|_| Ok(User { id: 1, name: "Alice".to_string() }));

    // Inject the mock instead of a real database.
    let service = UserService { db: mock_db };
    assert_eq!(service.get_user_name(1).unwrap(), "Alice");
}
```

### Sleep

Never use `sleep` in tests, even for a few milliseconds. Real sleeping slows down your test suite and introduces flakiness due to timing issues.

If you need to test code that calls `sleep`, use `tokio::time::pause()` and `tokio::time::advance()` to mock time:

```rust
#[tokio::test]
async fn test_delayed_operation() {
    tokio::time::pause(); // Pause real time

    let handle = tokio::spawn(function_with_five_second_sleep());
    tokio::time::advance(Duration::from_secs(5)).await;
    assert_eq!(handle.await.unwrap(), "foo");
}
```

## Error Handling & Safety

Use `Result<T, E>` for recoverable errors and `panic!` only for bugs.
Use `todo!` for to-be-completed code sections.
Don't use `unreachable!` and `unimplemented!`, as they are very similar in meaning to `panic!` and `todo!` and it's not worth the headache of figuring which one fits better.

### expect message

Write `expect` messages for code readers, not for the program runner. Explain _why_ the error should never happen, not _what_ is the error. State the invariant rather than saying that it broke.

```rust
// BAD
// This message is written for the program runner, and explains what is the error.
std::env::var("IMPORTANT_PATH").expect("Failed to get IMPORTANT_PATH");

// GOOD
// This message is written for the code reader, and explains why the error should never happen.
std::env::var("IMPORTANT_PATH").expect("IMPORTANT_PATH is set by wrapper_script.sh at startup");
```

### Unnecessary panics
If possible, avoid using code that can panic, and the only reason it doesn't panic is because of a check done above it. Instead, integrate both the check and the panicking code to one code that does both.

```rust
// BAD
if my_vec.is_empty() {
    return;
}
let x = my_vec[0]; // This panics if my_vec is empty

// GOOD
let Some(x) = my_vec.front() else {
    return;
}
```

### Error Types

Use `thiserror` for library errors and `anyhow` for binary errors.

Whenever possible, implement `PartialEq` on error enums, otherwise it's not possible to `assert_eq` on `Result`s with that error.

**Error messages formatting**: [use _lowercase_ sentences, _without_ punctuation](https://doc.rust-lang.org/stable/std/error/trait.Error.html): nested errors typically string-interpolate the inner error.

### Assertions

Prefer `expect` with useful information rather than unwrap, and use `unwrap` either in tests, or in absolutely trivial, bulletproof scenarios.

Don't run any non-const code inside `expect`, it is evaluated eagerly which can have unexpected side effects, or unnecessary string allocations. See also [or vs or else](#foo_or-vs-foo_or_else) section.

### Return Values

Always check return values from functions/methods, especially when modifying collections, which typically return an `Option` or a `bool`.

```rust
// BAD
map.insert(foo);

// GOOD
let result = map.insert(foo);
assert!(result, "dup-check was done previously in ...")
```

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

### Unnamed Function Parameters

Make an API that enforces the call-site parameter values to appear with meaningful names, unless the meaning of the values is trivial.

```rust
// GOOD - The meaning of `true` is clear enough on its own
set_operation_success_status(true)

// BAD - What do the values of the parameters mean?
create_transaction(true, false)
find_prime_numbers(None)
```

The tools you have at your disposal are:
1. Define enum for each parameter
```rust
create_transaction(DryRunStatus::Enabled, ExecutionStatus::Disabled)
find_prime_numbers(Logging::Disabled)
```
2. Define a struct for all the function parameters
```rust
create_transaction(CreateTransactionArgs { is_dry_run: true, is_executable: false })
```
3. If you prefer not to change the API or can't, add names at the callsite by defining variables. Note that the compiler won't enforce adjusting the callsite when the API changes.
```rust
let is_dry_run = true;
let is_executable = false;
create_transaction(is_dry_run, is_executable)
```

### Wide Parameter Type

Function parameters should use the most general type that satisfies the function's needs — the type that accepts the widest set of callers without requiring them to convert. In particular:

- `&[T]` over `Vec<T>`
- `&str` over `String`
- `Option<&T>` over `&Option<T>` (callers holding `&Option<T>` can pass `.as_ref()`, but not the reverse)

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

## Types

### Type Safety

Define types that wrap primitive types (usize, f64, bool, etc.) for frequently used struct members, function arguments, and return values.

This prevents bug where you accidentally mix up values.

```rust
// BAD
fn foo(nonce: u64) -> u64 {...}
fn bar(nonce: u64) -> u64 {...}
fn generate_nonce() -> u64 {...}

bar(foo(generate_nonce()));

// GOOD
struct Nonce(u64);
struct Timestamp(u64);
fn foo(nonce: Nonce) -> Nonce {...}
fn bar(nonce: Nonce) -> Timestamp {...} // Now it's clearer what bar returns
fn generate_nonce() -> Nonce {...}

bar(foo(generate_nonce()));
foo(bar(foo(generate_nonce()))); // This won't compile, which is good. It would've compiled before.

// ALSO BAD - This doesn't enforce anything
type Nonce = u64;
type Timestamp = u64;

// Define functions as before

foo(bar(foo(generate_nonce()))); // This will compile, even though it shouldn't.
```

### Newtypes

If newtyping as a "pass-through" wrapper (`Foo(pub u64)`), add `Deref` and `DerefMut` in order to avoid patterns of `foo.0.0.1`.

```rust
// BAD
struct Foo(pub u64);
...
if foo.0.0 > 0 {...}

// GOOD
struct Foo(u64)

impl Deref for Foo {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Foo {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
...
if *foo > 0 {...}
```

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

## External Dependencies

If you want to use a crate your repo doesn't currently use, you need an approval from your group manager. This is true even if the repo transitively depends on this crate already.

Be conservative with helper crates, and especially avoid single-use crates --- external deps increase our compilation time, and increase potential of version clashes and sudden breaking changes (not all crates adhere to Semver properly).

### Unmaintained Dependencies

Recommended checklist before adding a new dependency:

-   Last commit should be less than a month ago
-   Small crates should have at least ~100 github stars, large ones should have ~1000.
-   Skim through the README for important notes, ESPECIALLY look for "no longer maintained" or "archived" notices.
-   If there are indications for the crate no longer being maintained, search for "maintained" or "dead" in the crate's issues. Or see if recent pull-requests received any attention from the maintainers.
-   Some crates were integrated into std and are now dead. Use the std version instead. e.g: `lazy_static`, `async_trait`. If you see usages of those crates, help us improve the codebase and replace those crates with the std equivalent.


## Miscellaneous
### Async

Mostly avoid `join!`, as it doesn't short-circuit on errors in one of the tasks: use safe alternatives like `try_join!` (there are others that are also fine).

Mostly avoid `select!` unless you are certain the tasks are cancel-safe, this can lead to leaks and deadlocks: either spawn the tasks separately (`JoinHandle` is cancel-safe) or use safe alternatives like `FuturesUnordered` or `JoinSet`.

When spawning a task, you _must_ maintain the handle returned from the executor and make sure it completes successfully --- otherwise the task will _detach_ from the process.
### Macros

Only use macros if non-macro alternatives are not feasible and when _really_ necessary (like when writing a domain-specific language).

**Rationale**: [the more expressive the macro is, the less it can be reasoned about](https://matklad.github.io/2021/02/14/for-the-love-of-macros.html); well-structured non-macro code doesn't suffer from this.

For example: using parametrization in a test (using rstest) can make tests harder to reason about and debug in some cases, when the parametrization is very complicated and involves complex types. In those cases one should consider a free-function that performs the test, and several one-line tests that call it with different args, rather than forcing a parameterized solution.

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

### `foo_or` vs `foo_or_else`

Methods that end with `_or`, like `unwrap_or`, are typically eagerly evaluated, meaning `foo.unwrap_or(expensive_calculation_of_foo())` will always be evaluated, even if the original value is unwrapped --- use `_or_else` methods for lazy evaluations, which is typically what you want.
The same also holds for `expect`, and in general for all methods that take a non-closure parameter.

### Maps/Sets

Avoid iterating `HashSet` and `HashMap`, this is not deterministic in rust. Either use `BTree{Map,Set}` or `Index{Map,Set}`, and prefer `BTree` when you can, as the `Index` version has O(n) removals (it uses a `Vec` for storage).

### Allocations

Use `Vec::with_capacity()` when size is known to avoid reallocations.

### Match Ergonomics

Don't use `ref` keyword, it's considered legacy, as all of its uses can be replaced more concisely with `&` on the RHS.

```rust
let maybe_name = Some(String::from("Alice"));

/// ERROR
if let Some(n) = maybe_name {
    println!("Hello, {n}");
};
println!("{:?}", maybe_name); // BUG - `maybe_name` was consumed in the `if let`.

/// BAD
if let Some(ref n) = maybe_name {
    println!("Hello, {n}");
};
println!("{:?}", maybe_name);

// GOOD
if let Some(n) =  &maybe_name { // `n` is of type `&String`.
    println!("Hello, {n}");
}

println!("{:?}", maybe_name);
```

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

