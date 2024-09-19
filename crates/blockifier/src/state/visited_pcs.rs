use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;

use starknet_api::core::ClassHash;

use super::state_api::State;

/// This trait is used in `CachedState` to record visited pcs of an entry point call. This allows
/// flexible storage of program counters returned from cairo vm trace.
///
/// # Object Safety
///
/// This trait uses associated types instead of generics because only one implementation of the
/// trait is required. Also, using associated types reduces the number of parameters required to be
/// specified.
/// The use of associated types makes the trait implementation not [object safe](https://doc.rust-lang.org/reference/items/traits.html#object-safety).
///
/// Self Bounds
///
/// - [`Default`] is required to allow a default instantiation of `CachedState`.
/// - [`Debug`] is required for compatibility with other structs which derive `Debug`.
pub trait VisitedPcs
where
    Self: Default + Debug,
{
    /// This is the type which contains visited program counters.
    ///
    /// [`Clone`] is required to allow ownership of data throught cloning when receiving references
    /// from one of the trait methods.
    type Pcs: Clone;

    /// Constructs a concrete implementation of the trait.
    fn new() -> Self;

    /// This function records the program counters returned by the cairo vm trace.
    ///
    /// The elements of the vector `pcs` match the type of field `pc` in
    /// [`cairo_vm::vm::trace::trace_entry::RelocatedTraceEntry`]
    fn insert(&mut self, class_hash: &ClassHash, pcs: &[usize]);

    /// This function extends the program counters in `self` with those from another instance.
    ///
    /// It is used to transfer the visited program counters from one object to another.
    fn extend(&mut self, class_hash: &ClassHash, pcs: &Self::Pcs);

    /// This function returns an iterator of `VisitedPcs`.
    ///
    /// One tuple is returned for each class hash recorded in `self`.
    fn iter(&self) -> impl Iterator<Item = (&ClassHash, &Self::Pcs)>;

    /// Get the recorded visited program counters for a specific `class_hash`.
    fn entry(&mut self, class_hash: ClassHash) -> Entry<'_, ClassHash, Self::Pcs>;

    /// Marks the given `pcs` values as visited for the given class hash.
    fn add_visited_pcs(state: &mut dyn State, class_hash: &ClassHash, pcs: Self::Pcs);

    /// This function transforms the internal representation of program counters into a set.
    fn to_set(pcs: Self::Pcs) -> HashSet<usize>;
}

/// [`VisitedPcsSet`] is the default implementation of the trait [`VisitedPcs`]. All visited program
/// counters are inserted in a set and grouped by class hash.
///
/// This is also the structure used by the `native_blockifier`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct VisitedPcsSet(HashMap<ClassHash, HashSet<usize>>);
impl VisitedPcs for VisitedPcsSet {
    type Pcs = HashSet<usize>;

    fn new() -> Self {
        VisitedPcsSet(HashMap::default())
    }

    fn insert(&mut self, class_hash: &ClassHash, pcs: &[usize]) {
        self.0.entry(*class_hash).or_default().extend(pcs);
    }

    fn extend(&mut self, class_hash: &ClassHash, pcs: &Self::Pcs) {
        self.0.entry(*class_hash).or_default().extend(pcs);
    }

    fn iter(&self) -> impl Iterator<Item = (&ClassHash, &Self::Pcs)> {
        self.0.iter()
    }

    fn entry(&mut self, class_hash: ClassHash) -> Entry<'_, ClassHash, HashSet<usize>> {
        self.0.entry(class_hash)
    }

    fn add_visited_pcs(state: &mut dyn State, class_hash: &ClassHash, pcs: Self::Pcs) {
        state.add_visited_pcs(*class_hash, &Vec::from_iter(pcs));
    }

    fn to_set(pcs: Self::Pcs) -> HashSet<usize> {
        pcs
    }
}
