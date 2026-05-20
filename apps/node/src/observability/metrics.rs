//! Prometheus metric registry.

use prometheus::{Encoder, IntCounter, Registry, TextEncoder};

/// Process-wide metrics handle.
#[derive(Debug, Clone)]
pub struct Metrics {
    /// Registry shared with the axum exporter.
    pub registry: Registry,
    /// Count of `consensus::Event`s the orchestrator processed.
    pub events_processed: IntCounter,
    /// Count of `consensus::Action`s the orchestrator dispatched.
    pub actions_dispatched: IntCounter,
    /// Inbound gossip events dropped because the orchestrator queue was full.
    pub events_dropped: IntCounter,
    /// Outbound broadcast actions dropped because the swarm queue was full.
    pub actions_dropped: IntCounter,
}

impl Metrics {
    /// Create a fresh registry plus its standard counter set.
    pub fn new() -> anyhow::Result<Self> {
        let registry = Registry::new();
        let events_processed = IntCounter::new(
            "node_events_processed_total",
            "Number of consensus events processed by the orchestrator",
        )?;
        let actions_dispatched = IntCounter::new(
            "node_actions_dispatched_total",
            "Number of consensus actions dispatched by the orchestrator",
        )?;
        registry.register(Box::new(events_processed.clone()))?;
        let events_dropped = IntCounter::new(
            "node_events_dropped_total",
            "Inbound consensus events dropped due to a full orchestrator queue",
        )?;
        let actions_dropped = IntCounter::new(
            "node_actions_dropped_total",
            "Outbound broadcast actions dropped due to a full swarm queue",
        )?;
        registry.register(Box::new(actions_dispatched.clone()))?;
        registry.register(Box::new(events_dropped.clone()))?;
        registry.register(Box::new(actions_dropped.clone()))?;
        Ok(Self {
            registry,
            events_processed,
            actions_dispatched,
            events_dropped,
            actions_dropped,
        })
    }

    /// Encode the current state of the registry as prometheus text.
    pub fn render(&self) -> anyhow::Result<String> {
        let encoder = TextEncoder::new();
        let mfs = self.registry.gather();
        let mut buf = Vec::new();
        encoder.encode(&mfs, &mut buf)?;
        Ok(String::from_utf8(buf)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_can_be_rendered_to_prometheus_text() {
        let m = Metrics::new().unwrap();
        m.events_processed.inc();
        let text = m.render().unwrap();
        assert!(text.contains("node_events_processed_total"));
    }
}
