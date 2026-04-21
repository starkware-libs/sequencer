/// Override default allocator.
#[macro_export]
macro_rules! set_global_allocator {
    () => {
        #[global_allocator]
        static ALLOC: $crate::tikv_jemallocator::Jemalloc = $crate::tikv_jemallocator::Jemalloc;
    };
}
