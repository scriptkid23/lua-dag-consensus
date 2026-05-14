//! `Clock` port. The simulator uses a virtual clock; the node uses tokio.

/// Monotonic clock readings in nanoseconds.
pub trait Clock: Send + Sync {
    /// Return the current monotonic time in nanoseconds since an
    /// implementation-defined epoch (e.g. process start).
    fn now_nanos(&self) -> u128;
}
