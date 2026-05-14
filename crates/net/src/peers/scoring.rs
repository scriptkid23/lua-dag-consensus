//! Per-peer score in `i32` "points". Negative score below
//! `BAN_THRESHOLD` triggers a ban.

/// Reasons for score changes (informational; not load-bearing).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reason {
    /// Peer delivered a valid message on time.
    GoodDelivery,
    /// Peer delivered late or duplicate.
    SlowDelivery,
    /// Peer delivered malformed payload.
    InvalidMessage,
    /// Peer is known to have equivocated.
    Equivocation,
}

/// Score below which a peer is banned.
pub const BAN_THRESHOLD: i32 = -500;

/// Per-peer score.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PeerScore {
    points: i32,
}

impl PeerScore {
    /// Neutral starting score.
    #[must_use]
    pub fn neutral() -> Self {
        Self { points: 0 }
    }

    /// Apply a delta.
    pub fn adjust(&mut self, delta: i32, _reason: Reason) {
        self.points = self.points.saturating_add(delta);
    }

    /// Current score.
    #[must_use]
    pub fn points(self) -> i32 {
        self.points
    }

    /// True iff score has crossed the ban threshold.
    #[must_use]
    pub fn is_banned(self) -> bool {
        self.points <= BAN_THRESHOLD
    }
}
