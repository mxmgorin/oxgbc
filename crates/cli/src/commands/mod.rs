//! CLI subcommands. Each command is its own module here; add new ones as
//! sibling files and re-export their entry point below.

pub mod bench;
pub mod check;
pub mod run;
pub mod score;

pub use bench::cmd_bench;
pub use check::cmd_check;
pub use run::cmd_run;
pub use score::cmd_score;
