//! Monotonic clock abstraction.
//!
//! The solver's anti-hang mechanism is a wall-clock `Instant` deadline: `solve`
//! records a start time, each solver part computes `start + Duration` as its
//! deadline, and the hot loops compare `now() >= deadline` to bail out of a
//! runaway search. On native this is `std::time::Instant`.
//!
//! The `wasm32-unknown-unknown` target has **no operating system clock**
//! (`std::time::Instant::now()` is unusable there), so under
//! `target_arch = "wasm32"` we read `performance.now()` — a monotonic millisecond
//! source available in browsers and Web Workers — through `wasm-bindgen`.
//!
//! The rest of the solver is unchanged: it only ever calls `now()`, adds a
//! `std::time::Duration`, compares two instants, or reads `elapsed().as_millis()`.
//! `Duration` itself is a plain `{secs, nanos}` struct and remains usable on
//! wasm, so `std::time::Duration` stays untouched throughout the codebase.

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::Instant;

#[cfg(target_arch = "wasm32")]
mod wasm_impl {
    use std::ops::Add;
    use std::time::Duration;

    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = performance, js_name = now)]
        fn performance_now() -> f64;
    }

    /// A `std::time::Instant`-shaped monotonic clock backed by `performance.now()`
    /// (milliseconds since the page/worker time origin — monotonic, no wall-clock
    /// jumps). Implements exactly the operations the solver uses: `now()`,
    /// `Add<Duration>`, ordering, and `elapsed()`.
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
    pub struct Instant(f64);

    impl Instant {
        pub fn now() -> Self {
            Instant(performance_now())
        }

        pub fn elapsed(&self) -> Duration {
            Duration::from_secs_f64((performance_now() - self.0) / 1000.0)
        }
    }

    impl Add<Duration> for Instant {
        type Output = Instant;
        fn add(self, rhs: Duration) -> Instant {
            Instant(self.0 + rhs.as_secs_f64() * 1000.0)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm_impl::Instant;
