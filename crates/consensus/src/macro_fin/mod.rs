//! L3 macro-finality (whitepaper §9).

pub mod aggregation;
pub mod checkpoint;
pub mod macro_qc;
pub mod proposer;
pub mod two_chain;
pub mod vote_book;
pub mod window;

pub use aggregation::{AggregationMode, Ke, select_mode};
pub use checkpoint::CheckpointBuilder;
pub use macro_qc::MacroQcAssembler;
pub use proposer::ProposerSchedule;
pub use two_chain::TwoChainRule;
pub use vote_book::VoteBook;
pub use window::MacroWindow;
