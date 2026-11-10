//! The scheduled calendar sync job (#58): a Cloud Run Job that syncs every
//! due imported calendar and republishes stuck calendar emails.
#![deny(missing_docs)]

/// One run of the job.
pub mod run;
#[cfg(test)]
mod run_tests;

pub use run::{JobConfig, JobDeps, RunSummary, run};
