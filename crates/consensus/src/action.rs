//! `Action` — every side-effect the state machine can request.

use borsh::{BorshDeserialize, BorshSerialize};
use types::{
    macros::{MacroProposal, MacroQc},
    micro::MicroQc,
    primitives::{BlobId, ValidatorId},
    slashing::SlashEvidence,
};

use crate::{
    api::tier::BlobStatus,
    event::{BlsPartial, SubnetAggregate, TimerId},
};

/// All outputs from the consensus state machine.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum Action {
    /// Broadcast a local MicroQc.
    BroadcastMicroQc(MicroQc),
    /// Broadcast a macro proposal.
    BroadcastMacroProposal(MacroProposal),
    /// Broadcast a partial BLS signature.
    BroadcastBlsPartial(BlsPartial),
    /// Broadcast a subnet aggregate.
    BroadcastSubnetAggregate(SubnetAggregate),
    /// Broadcast a complete macro QC.
    BroadcastMacroQc(MacroQc),
    /// Schedule a new timer; host emits `Event::TimerFired(id)` after the delay.
    ScheduleTimer {
        /// Identifier the host must echo back.
        id: TimerId,
        /// Delay in nanoseconds (host converts to `Duration`).
        delay_nanos: u128,
    },
    /// Cancel a previously scheduled timer.
    CancelTimer(TimerId),
    /// Persist a finalized MacroQc.
    PersistMacroQc(MacroQc),
    /// Emit slashing evidence to gossip + storage.
    EmitSlashEvidence {
        /// The offender.
        offender: ValidatorId,
        /// Evidence payload.
        evidence: SlashEvidence,
    },
    /// Update the externally-visible status of a blob.
    UpdateBlobStatus {
        /// Blob whose status changes.
        blob: BlobId,
        /// New status.
        status: BlobStatus,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use borsh::to_vec;

    #[test]
    fn schedule_timer_round_trips() {
        let a = Action::ScheduleTimer {
            id: TimerId(1),
            delay_nanos: 250_000_000,
        };
        let bytes = to_vec(&a).unwrap();
        let a2: Action = borsh::from_slice(&bytes).unwrap();
        assert_eq!(a, a2);
    }
}
