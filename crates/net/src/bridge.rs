//! libp2p ↔ consensus translation.
//!
//! Direction of flow:
//!
//! - inbound  : libp2p event  → `consensus::Event`  → `events_tx`
//! - outbound : `consensus::Action` → libp2p publish / RPC send
//!
//! No protocol semantics live here: every variant of `consensus::Event`
//! and `consensus::Action` has exactly one translation function. Real
//! algorithm decisions are still inside `crates/consensus`.

use consensus::{action::Action, event::Event};
use tokio::sync::mpsc;
use tracing::warn;

use crate::error::Result;

/// Handle returned to the host. Sending an `Action` here causes the
/// bridge to translate it into libp2p publish / RPC operations.
#[derive(Clone, Debug)]
pub struct BridgeHandle {
    actions_tx: mpsc::Sender<Action>,
}

impl BridgeHandle {
    /// Submit an outbound action.
    pub async fn apply_action(&self, action: Action) -> Result<()> {
        self.actions_tx
            .send(action)
            .await
            .map_err(|_| crate::error::Error::BridgeClosed)
    }
}

/// Bridge state owned by the host.
#[derive(Debug)]
pub struct Bridge {
    /// Inbound event sender (handed to `consensus::StateMachine` driver).
    pub events_tx: mpsc::Sender<Event>,
    /// Outbound action receiver consumed by the swarm loop.
    pub actions_rx: mpsc::Receiver<Action>,
}

impl Bridge {
    /// Construct a new bridge pair plus its [`BridgeHandle`].
    ///
    /// `events_capacity` and `actions_capacity` size the underlying
    /// tokio mpsc channels.
    #[must_use]
    pub fn new(events_capacity: usize, actions_capacity: usize) -> (Self, BridgeHandle) {
        // NOTE: the receiver paired with `events_tx` is dropped here; the real
        // receiver is owned by the host (plan 06). The host constructs its own
        // pair and hands `events_tx` to the bridge. This default constructor
        // is for tests where the host isn't present.
        let (events_tx, _) = mpsc::channel(events_capacity);
        let (actions_tx, actions_rx) = mpsc::channel(actions_capacity);
        let bridge = Self {
            events_tx,
            actions_rx,
        };
        let handle = BridgeHandle { actions_tx };
        (bridge, handle)
    }

    /// Construct a bridge with externally-supplied channels.
    #[must_use]
    pub fn with_channels(
        events_tx: mpsc::Sender<Event>,
        actions_capacity: usize,
    ) -> (Self, BridgeHandle) {
        let (actions_tx, actions_rx) = mpsc::channel(actions_capacity);
        (
            Self {
                events_tx,
                actions_rx,
            },
            BridgeHandle { actions_tx },
        )
    }

    /// Translate one outbound `Action`. Returns `Ok(())` on success.
    ///
    /// Skeleton: logs the action and returns. Plan 06 replaces the body
    /// with publish/RPC calls on the libp2p swarm that can fail.
    #[allow(clippy::unnecessary_wraps)]
    pub fn translate_action(action: &Action) -> Result<()> {
        match action {
            Action::BroadcastMicroQc(_)
            | Action::BroadcastMacroProposal(_)
            | Action::BroadcastBlsPartial(_)
            | Action::BroadcastSubnetAggregate(_)
            | Action::BroadcastMacroQc(_) => {
                // TODO(plan 06): map to `Topic` + `gossip::codec::encode_action_payload`
                // + swarm publish. Skeleton: warn so silent drops are visible.
                warn!(target: "net::bridge", "skeleton: dropping outbound broadcast action");
                Ok(())
            }
            Action::ScheduleTimer { .. } | Action::CancelTimer(_) => {
                // Timers are host-local; not a network concern.
                Ok(())
            }
            Action::PersistMacroQc(_)
            | Action::EmitSlashEvidence { .. }
            | Action::UpdateBlobStatus { .. } => {
                // Storage / API — not a network concern.
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use consensus::{api::tier::BlobStatus, event::TimerId};
    use types::primitives::BlobId;

    #[test]
    fn translate_action_is_total_in_skeleton() {
        // All variants must return Ok(()) under the skeleton bridge.
        Bridge::translate_action(&Action::ScheduleTimer {
            id: TimerId(1),
            delay_nanos: 1,
        })
        .unwrap();
        Bridge::translate_action(&Action::CancelTimer(TimerId(1))).unwrap();
        Bridge::translate_action(&Action::UpdateBlobStatus {
            blob: BlobId([0; 32]),
            status: BlobStatus::Accepted,
        })
        .unwrap();
    }

    #[tokio::test]
    async fn handle_propagates_actions_and_drops_on_close() {
        let (events_tx, _events_rx) = mpsc::channel(1);
        let (bridge, handle) = Bridge::with_channels(events_tx, 4);
        handle
            .apply_action(Action::CancelTimer(TimerId(0)))
            .await
            .unwrap();
        let mut bridge = bridge;
        let got = bridge.actions_rx.recv().await.unwrap();
        assert_eq!(got, Action::CancelTimer(TimerId(0)));

        // Drop bridge → receiver gone → handle sees BridgeClosed.
        drop(bridge);
        let err = handle
            .apply_action(Action::CancelTimer(TimerId(1)))
            .await
            .unwrap_err();
        assert!(matches!(err, crate::error::Error::BridgeClosed));
    }
}
