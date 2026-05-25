//! Macro equivocation detector (100 % slash).

use crypto::{bls::PublicKey, bls::sign::verify as bls_verify, hash::dst};
use types::slashing::MacroEquivocation;

use crate::{error::Result, macro_fin::messages};

/// Verify a macro-equivocation evidence bundle.
pub fn verify(ev: &MacroEquivocation, set: &types::validator::ValidatorSet) -> Result<()> {
    let entry = set
        .entries
        .iter()
        .find(|e| e.id == ev.validator)
        .ok_or_else(|| crate::Error::InvalidConfig("unknown validator".into()))?;
    let pk = PublicKey::from_bytes(&entry.bls_pubkey)
        .map_err(|_| crate::Error::InvalidConfig("invalid bls pubkey".into()))?;

    if ev.a.0.height != ev.b.0.height || ev.a.0.hash == ev.b.0.hash {
        return Err(crate::Error::InvalidConfig(
            "equivocation checkpoints must share height and differ in hash".into(),
        ));
    }

    for (cp, sig) in [&ev.a, &ev.b] {
        let msg = messages::proposer_message(&ev.validator, cp);
        bls_verify(&pk, dst::MACRO_PROPOSER_SIG, &msg, sig)
            .map_err(|_| crate::Error::InvalidConfig("invalid proposer sig".into()))?;
    }
    Ok(())
}

/// Build evidence from two conflicting proposals at the same height.
#[must_use]
pub fn detect(
    validator: types::primitives::ValidatorId,
    a: types::macros::MacroProposal,
    b: types::macros::MacroProposal,
) -> MacroEquivocation {
    MacroEquivocation {
        validator,
        a: (a.checkpoint, a.proposer_sig),
        b: (b.checkpoint, b.proposer_sig),
    }
}
