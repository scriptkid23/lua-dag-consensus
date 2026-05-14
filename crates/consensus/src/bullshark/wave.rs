//! Wave numbering (4 rounds per wave): `4w, 4w+1, 4w+2, 4w+3`.

use types::primitives::Round;

/// A Bullshark wave (4 consecutive rounds).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct WaveId(pub u64);

impl WaveId {
    /// First round in this wave.
    #[must_use]
    pub fn first_round(self) -> Round {
        Round(self.0 * 4)
    }

    /// Last round in this wave.
    #[must_use]
    pub fn last_round(self) -> Round {
        Round(self.0 * 4 + 3)
    }

    /// Wave containing `round`.
    #[must_use]
    pub fn of_round(round: Round) -> Self {
        Self(round.0 / 4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wave_bounds() {
        let w = WaveId(2);
        assert_eq!(w.first_round(), Round(8));
        assert_eq!(w.last_round(), Round(11));
        assert_eq!(WaveId::of_round(Round(10)), WaveId(2));
    }
}
