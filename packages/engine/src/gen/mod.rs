/// Manually created mod.rs file as flatc 2.0.0 does not generate one although it seems that work is
/// underway to do so automatically
pub mod batch_generated;
pub use self::batch_generated::*;

pub mod init_generated;

pub use self::init_generated::*;

pub mod metaversion_generated;
pub use self::metaversion_generated::*;

pub mod new_simulation_run_generated;
pub use self::new_simulation_run_generated::*;

pub mod package_config_generated;
pub use self::package_config_generated::*;

pub mod package_error_generated;
pub use self::package_error_generated::*;

pub mod runner_error_generated;
pub use self::runner_error_generated::*;

pub mod runner_errors_generated;
pub use self::runner_errors_generated::*;

pub mod runner_inbound_msg_generated;
pub use self::runner_inbound_msg_generated::*;

pub mod runner_outbound_msg_generated;
pub use self::runner_outbound_msg_generated::*;

pub mod runner_warning_generated;
pub use self::runner_warning_generated::*;

pub mod runner_warnings_generated;
pub use self::runner_warnings_generated::*;

pub mod serialized_generated;
pub use self::serialized_generated::*;

pub mod shared_context_generated;
pub use self::shared_context_generated::*;

pub mod sync_context_batch_generated;
pub use self::sync_context_batch_generated::*;

pub mod sync_state_generated;
pub use self::sync_state_generated::*;

pub mod sync_state_interim_generated;
pub use self::sync_state_interim_generated::*;

pub mod sync_state_snapshot_generated;
pub use self::sync_state_snapshot_generated::*;

pub mod target_generated;
pub use self::target_generated::*;

pub mod user_error_generated;
pub use self::user_error_generated::*;

pub mod user_errors_generated;
pub use self::user_errors_generated::*;

pub mod user_warning_generated;
pub use self::user_warning_generated::*;

pub mod user_warnings_generated;
pub use self::user_warnings_generated::*;