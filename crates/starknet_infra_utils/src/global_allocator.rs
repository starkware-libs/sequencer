/// Override default allocator.
#[macro_export]
macro_rules! set_global_allocator {
    () => {
        #[global_allocator]
        static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
    };
}
