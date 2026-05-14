//! Casper-FFG surround-vote detector.

use types::{primitives::ValidatorId, slashing::SurroundVote};

use crate::{error::Result, macro_fin::vote_book::VoteBook};

/// Scan `book` for a surround vote committed by `validator`. Skeleton
/// returns `Ok(None)`.
pub fn scan_for_surround(
    _book: &VoteBook,
    _validator: &ValidatorId,
) -> Result<Option<SurroundVote>> {
    Ok(None)
}
