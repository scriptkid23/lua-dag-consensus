//! Finality-latency aggregation (p50, p95).

/// Latency aggregator.
#[derive(Debug, Default)]
pub struct LatencyStats {
    samples_ns: Vec<u64>,
}

impl LatencyStats {
    /// New empty aggregator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a finality latency (nanoseconds).
    pub fn record(&mut self, ns: u64) {
        self.samples_ns.push(ns);
    }

    /// Compute `(p50, p95)` in nanoseconds. Returns `(0, 0)` when empty.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    pub fn percentiles(&self) -> (u64, u64) {
        if self.samples_ns.is_empty() {
            return (0, 0);
        }
        let mut s = self.samples_ns.clone();
        s.sort_unstable();
        let idx =
            |p: f64| -> usize { (((s.len() as f64 - 1.0) * p).round() as usize).min(s.len() - 1) };
        (s[idx(0.50)], s[idx(0.95)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_with_known_distribution() {
        let mut s = LatencyStats::new();
        for v in 1..=100u64 {
            s.record(v);
        }
        let (p50, p95) = s.percentiles();
        assert!((50..=51).contains(&p50));
        assert!((95..=96).contains(&p95));
    }
}
