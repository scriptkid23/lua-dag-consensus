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
    /// Inactivity leak notifications applied locally.
    pub inactivity_leak_emitted: IntCounter,
    /// Certified vertices rejected by the L1 cert verifier (07a).
    pub vertex_cert_rejected: IntCounter,
    /// Inbound blob chunks stored by custody.
    pub blob_chunks_received: IntCounter,
    /// Blob chunks published locally via submit.
    pub blob_chunks_published: IntCounter,
    /// Blobs that reached full local chunk custody.
    pub blob_available: IntCounter,
    /// Inbound blob chunks rejected by the store.
    pub blob_chunk_rejected: IntCounter,
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
        let inactivity_leak_emitted = IntCounter::new(
            "node_inactivity_leak_emitted_total",
            "NotifyInactivityLeak actions applied by the node",
        )?;
        let vertex_cert_rejected = IntCounter::new(
            "node_vertex_cert_rejected_total",
            "Certified vertices rejected by the L1 BLS certificate verifier",
        )?;
        registry.register(Box::new(actions_dispatched.clone()))?;
        registry.register(Box::new(events_dropped.clone()))?;
        registry.register(Box::new(actions_dropped.clone()))?;
        registry.register(Box::new(inactivity_leak_emitted.clone()))?;
        registry.register(Box::new(vertex_cert_rejected.clone()))?;
        let blob_chunks_received = IntCounter::new(
            "node_blob_chunks_received_total",
            "Blob chunks ingested by custody from gossip or local publish",
        )?;
        let blob_chunks_published = IntCounter::new(
            "node_blob_chunks_published_total",
            "Blob chunks published locally via submit",
        )?;
        let blob_available = IntCounter::new(
            "node_blob_available_total",
            "Blobs that reached full local chunk custody",
        )?;
        let blob_chunk_rejected = IntCounter::new(
            "node_blob_chunk_rejected_total",
            "Inbound blob chunks rejected by the chunk store",
        )?;
        registry.register(Box::new(blob_chunks_received.clone()))?;
        registry.register(Box::new(blob_chunks_published.clone()))?;
        registry.register(Box::new(blob_available.clone()))?;
        registry.register(Box::new(blob_chunk_rejected.clone()))?;
        Ok(Self {
            registry,
            events_processed,
            actions_dispatched,
            events_dropped,
            actions_dropped,
            inactivity_leak_emitted,
            vertex_cert_rejected,
            blob_chunks_received,
            blob_chunks_published,
            blob_available,
            blob_chunk_rejected,
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
