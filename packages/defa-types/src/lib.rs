pub mod fuzz_helpers;
pub mod addr_validate_bypass;
pub mod drawdown;
pub mod pool_params;
pub mod pool_state;

pub use drawdown::{Drawdown, DrawdownStatus};
pub use pool_params::PoolInitParams;
pub use pool_state::PoolState;

pub const SECONDS_PER_DAY: u64 = 86_400;
pub const SECONDS_PER_WEEK: u64 = 7 * SECONDS_PER_DAY;
pub const APY_DAYS_BASIS: u64 = 360;
pub const BPS_DENOMINATOR: u64 = 10_000;
pub const UNIX_EPOCH_DAY_OF_WEEK_OFFSET: u64 = 4;
pub const MONDAY: u8 = 1;
