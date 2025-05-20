// This struct is used to abort the program if a panic occurred in a place where it could not be
// handled.
pub struct AbortIfPanic;

impl Drop for AbortIfPanic {
    fn drop(&mut self) {
        eprintln!("detected unexpected panic; aborting");
        std::process::abort();
    }
}

impl AbortIfPanic {
    pub fn release(self) {
        std::mem::forget(self);
    }
}
