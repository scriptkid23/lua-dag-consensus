//! Data-availability challenge skeleton (07c).

pub mod challenge;

pub use challenge::{
    verify_availability_response, AvailabilityChallenge, AvailabilityResponse, ChallengeError,
};
