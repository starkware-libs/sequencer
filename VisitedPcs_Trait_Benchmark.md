# Adding the `VisitedPcs` Trait

The state of the blockifier as of commit
`16e8954934c3f71a52a9d0688f5a18df33009d08` doesn't store the complete vector of
visited program counters for each entry-point in an invoke transaction. Instead,
visited program counters are pushed into a `HashSet`. Unfortunately this limits
the ability to perform profiling operations, as many require a record of the
full trace returned from the `cairo-vm`.

In order to enable more in-depth tracing use-cases, we have introduced the
`VisitedPcs` trait which allows the user to process the visited program counters
as they see fit.

## Before Changes

Visited program counters are kept in the `CachedState` structure as shown below:

```rust
#[derive(Debug)]
pub struct CachedState<S: StateReader> {
    pub state: S,
    // Invariant: read/write access is managed by CachedState.
    // Using interior mutability to update caches during `State`'s immutable getters.
    pub(crate) cache: RefCell<StateCache>,
    pub(crate) class_hash_to_class: RefCell<ContractClassMapping>,
    /// A map from class hash to the set of PC values that were visited in the class.
    pub visited_pcs: HashMap<ClassHash, HashSet<usize>>,
}
```

This snippet has been extracted from commit
[16e8954934c3f71a52a9d0688f5a18df33009d08](https://github.com/reilabs/sequencer/blob/16e8954934c3f71a52a9d0688f5a18df33009d08/crates/blockifier/src/state/cached_state.rs#L36)

## After Changes

> [!NOTE]
> The new code is developed in the branch `visited_pcs_trait` and the
> current head of the branch is at commit
> [`391ac93e7ecabf3307e33318add5d95ca4366659`](https://github.com/reilabs/sequencer/blob/visited_pcs_trait/crates/blockifier/src/state/cached_state.rs#L37).
> This will change once these changes are merged in the main branch.

`VisitedPcs` is added as an additional generic parameter of `CachedState`.

```rust
#[derive(Debug)]
pub struct CachedState<S: StateReader, V: VisitedPcs> {
    pub state: S,
    // Invariant: read/write access is managed by CachedState.
    // Using interior mutability to update caches during `State`'s immutable getters.
    pub(crate) cache: RefCell<StateCache>,
    pub(crate) class_hash_to_class: RefCell<ContractClassMapping>,
    /// A map from class hash to the set of PC values that were visited in the class.
    pub visited_pcs: V,
}
```

An implementation of the trait `VisitedPcs` is included in the blockifier with
the name `VisitedPcsSet`. This mimics the existing `HashSet<usize>` usage of
this field. For test purposes, `CachedState` is instantiated using
`VisitedPcsSet`.

## Performance Considerations

Given the importance of the blockifier's performance in the Starknet ecosystem,
measuring the impact of adding the aforementioned `VisitedPcs` trait is very
important. The existing bechmark `transfers` doesn't cover operations that use
the `CachedState`, and therefore we have designed new ones as follows:

- `cached_state`: this benchmark tests the performance impact of populating
  `visited_pcs` (implemented using `VisitedPcsSet`) with a realistic amount of
  visited program counters. The size of the sets is taken from transaction
  `0x0177C9365875CAA840EA8F03F97B0E3A8EE8851A8B952BF157B5DBD4FECCB060` on
  mainnet. This transaction has been chosen randomly but there is no assurance
  that it's representative of the most common Starknet invoke transaction. This
  benchmark tests the write performance of visited program counters in the state
  struct.
- `execution`: this benchmark simulates a whole invoke transaction using a dummy
  contract.

## Performance Impact

The `bench.sh` script has been added to benchmark the performance impact of
these changes.

The benchmark results presented below were conducted under the following
conditions:

- **Operating System:** Debian 12 (Bookworm) running in a VMWare Workstation 17
  VM on Windows 10 22H2
- **Hardware:** i9-9900K @ 5.0 GHz, 64GB of RAM, Samsung 990 Pro NVMe SSD.
- **Rust Toolchain:** 1.78-x86_64-unknown-linux-gnu / rust 1.78.0 (9b00956e5
  2024-04-29).

The script was called as follows, but you may need to [adjust the commit
hashes](#after-changes) in question to reproduce these results:

```bash
bash scripts/bench.sh 16e8954934c3f71a52a9d0688f5a18df33009d08 391ac93e7ecabf3307e33318add5d95ca4366659
```

The noise threshold and confidence intervals are kept as per default
Criterion.rs configuration.

The results are as follows:

| Benchmark    | Time (ms) | Time change (%) | Criterion.rs report           |
| ------------ | --------- | --------------- | ----------------------------- |
| transfers    | 94.599    | +1.1246         | Change within noise threshold |
| execution    | 1.2644    | -0.3376         | No change in performance      |
| cached_state | 5.1233    | +10.121         | No change in performance      |

Criterion's inbuilt confidence analysis suggests that these results have no
statistical significant and do not represent real-world performance changes.
