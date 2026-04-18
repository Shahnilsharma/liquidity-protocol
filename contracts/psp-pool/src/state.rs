/// Per-user info for LP holders (for pro-rata yield logic)
#[cw_serde]
pub struct UserInfo {
    pub deposit_timestamp: u64,
    pub lp_balance: Uint128,
}

/// Map of user address to UserInfo
pub const USER_INFOS: Map<&Addr, UserInfo> = Map::new("user_infos");
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Timestamp, Uint128};
use cw_storage_plus::{Item, Map};
use defa_types::{PoolState, SECONDS_PER_DAY};

/// Minimum withdrawal delay: 2 minutes (120 seconds)
/// Provides flexibility for testing while maintaining security
pub const MIN_WITHDRAWAL_DELAY: u64 = 120;
/// Maximum withdrawal delay: 30 days (2,592,000 seconds)
pub const MAX_WITHDRAWAL_DELAY: u64 = 2_592_000;

/// Contract configuration stored in state
#[cw_serde]
pub struct Config {
    /// Denom of the stablecoin (e.g., "uzig" or TokenFactory denom)
    pub stablecoin_denom: String,
    /// Full denom of the LP token created by this contract
    /// Format: coin.{contract_address}.{subdenom}
    pub lp_full_denom: String,
    /// Admin address that can update config and manage vault funds
    /// Admin has exclusive rights to move funds to yield protocols
    pub admin: Addr,
    /// Withdrawal delay in seconds - IMMUTABLE after instantiation
    /// This provides security against flash attacks and unauthorized withdrawals
    pub withdrawal_delay: u64,
    /// Authorized CreditManager allowed to disburse principal to PSP.
    pub credit_manager: Option<Addr>,
    /// Investor APY in basis points (bps)
    pub investor_apy_bps: u64,
    /// YieldReserve contract address
    pub yield_reserve: Addr,
    /// Execution amount cap for backfill ceiling
    pub execution_amount: Uint128,
}

/// Vault statistics
#[cw_serde]
pub struct VaultState {
    /// Total stablecoin value owned by the vault (accounting value)
    /// This includes all deposited funds plus accrued yield
    /// Admin manages actual fund movements; this tracks the accounting total
    /// Formula: total_deposited = deposits + yield - withdrawals
    pub total_deposited: Uint128,
    /// Total LP tokens minted by this vault
    pub total_lp_minted: Uint128,
    /// Total amount of stablecoins locked in pending withdrawals
    /// (valued at the time of withdrawal request)
    pub total_pending_withdrawals: Uint128,
}

/// Pending withdrawal request
#[cw_serde]
pub struct PendingWithdrawal {
    /// FIFO queue ID assigned when request is created
    pub queue_id: u64,
    /// Amount of stablecoins to be withdrawn
    pub amount: Uint128,
    /// Request timestamp
    pub requested_at: Timestamp,
    /// Time when the withdrawal can be claimed
    pub release_time: Timestamp,
}

#[cw_serde]
pub struct QueueEntry {
    pub user: Addr,
    pub withdrawal_id: u64,
    pub requested_at: Timestamp,
}

/// Counter for generating unique withdrawal IDs per user
#[cw_serde]
pub struct WithdrawalCounter {
    pub next_id: u64,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const VAULT_STATE: Item<VaultState> = Item::new("vault_state");

/// Map of pending withdrawals: (user_address, withdrawal_id) -> PendingWithdrawal
pub const PENDING_WITHDRAWALS: Map<(&Addr, u64), PendingWithdrawal> =
    Map::new("pending_withdrawals");

/// Counter for each user's next withdrawal ID
pub const WITHDRAWAL_COUNTERS: Map<&Addr, WithdrawalCounter> = Map::new("withdrawal_counters");

/// Queue head pointer (next FIFO queue_id claimable when conditions are met).
pub const WITHDRAWAL_QUEUE_HEAD: Item<u64> = Item::new("withdrawal_queue_head");

/// Monotonic queue ID allocation pointer.
pub const WITHDRAWAL_QUEUE_NEXT: Item<u64> = Item::new("withdrawal_queue_next");

/// queue_id -> queue entry metadata.
pub const WITHDRAWAL_QUEUE: Map<u64, QueueEntry> = Map::new("withdrawal_queue");

/// (user, withdrawal_id) -> queue_id mapping for O(1) claim lookup.
pub const USER_WITHDRAWAL_QUEUE_ID: Map<(&Addr, u64), u64> =
    Map::new("user_withdrawal_queue_id");

/// Indexed LP holders used for on-chain yield payout enumeration.
pub const LP_HOLDERS: Map<&Addr, bool> = Map::new("lp_holders");

/// Current lifecycle state for this facility.
pub const POOL_STATE: Item<PoolState> = Item::new("pool_state");

/// Emergency pause flag. When true, all mutating user/admin operations are blocked.
pub const PAUSED: Item<bool> = Item::new("paused");

pub const CONTRACT_NAME: &str = "crates.io:defa-psp-pool";
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const MONDAY_WITHDRAWAL_WINDOW_DAY: u64 = 1;

pub fn day_of_week(timestamp_secs: u64) -> u64 {
    // Unix epoch started on Thursday (index 4 in [Sun=0..Sat=6]).
    let unix_epoch_day_of_week_offset = 4u64;
    let days_since_epoch = timestamp_secs / SECONDS_PER_DAY;
    (days_since_epoch + unix_epoch_day_of_week_offset) % 7
}
