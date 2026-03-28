pub mod audio;
pub mod extensions;
pub mod library;
pub mod playlists;
pub mod settings;

/// Convenience trait: lock a Mutex and convert the PoisonError to String.
/// Eliminates `.lock().map_err(|e| e.to_string())?` boilerplate across all commands.
pub trait LockExt<T> {
    fn acquire(&self) -> Result<std::sync::MutexGuard<'_, T>, String>;
}

impl<T> LockExt<T> for std::sync::Mutex<T> {
    fn acquire(&self) -> Result<std::sync::MutexGuard<'_, T>, String> {
        self.lock().map_err(|e| e.to_string())
    }
}
