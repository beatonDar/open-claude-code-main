//! Small cross-cutting utilities shared by the backend.
//!
//! Currently this exposes a single extension trait, [`LockSafe`], which
//! wraps `std::sync::Mutex::lock()` in a way that is robust to poisoning.
//! A poisoned mutex means some *previous* lock holder panicked while the
//! lock was held. The state protected by the mutex is still there — it
//! may be in a half-updated shape, but for every mutex we use in
//! `AppState` the invariants are trivial enough (a `Settings` struct, a
//! `bool`, a `HashMap` of pending confirms, a `HashMap` of watchers)
//! that continuing with the inner value is strictly better than panicking
//! the whole app and leaving the user with a dead window.
//!
//! Using `lock().unwrap()` instead turns a single panic (which Tauri
//! already caught and contained) into a cascading failure: every future
//! Tauri command that touches the same `Mutex` also panics, and the app
//! becomes unusable until the user restarts it.
 
use std::sync::{Mutex, MutexGuard};
 
pub trait LockSafe<T: ?Sized> {
    /// Acquire the mutex, recovering the inner guard even if a previous
    /// holder panicked. Prefer this over `lock().unwrap()` for locks
    /// held only to read/replace small, self-consistent state.
    fn lock_safe(&self) -> MutexGuard<'_, T>;
}
 
impl<T: ?Sized> LockSafe<T> for Mutex<T> {
    fn lock_safe(&self) -> MutexGuard<'_, T> {
        match self.lock() {
            Ok(g) => g,
            Err(poisoned) => {
                tracing::warn!(
                    "recovering from poisoned mutex (previous holder panicked)"
                );
                poisoned.into_inner()
            }
        }
    }
}
 
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
 
    #[test]
    fn lock_safe_returns_value_when_not_poisoned() {
        let m = Mutex::new(41u32);
        *m.lock_safe() += 1;
        assert_eq!(*m.lock_safe(), 42);
    }
 
    #[test]
    fn lock_safe_recovers_after_poisoning() {
        let m = Arc::new(Mutex::new(10u32));
        let m2 = m.clone();
        // Poison the mutex from another thread by panicking while it
        // is held.
        let _ = thread::spawn(move || {
            let mut g = m2.lock().unwrap();
            *g = 99;
            panic!("intentional for test");
        })
        .join();
        // Standard lock() would error; lock_safe() must recover.
        assert!(m.lock().is_err(), "mutex should be poisoned");
        assert_eq!(*m.lock_safe(), 99);
    }
}
