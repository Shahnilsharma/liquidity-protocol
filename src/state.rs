use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Timestamp, Uint128};
use cw_storage_plus::{Item, Map};

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
    /// Amount of stablecoins to be withdrawn
    pub amount: Uint128,
    /// Time when the withdrawal can be claimed
    pub release_time: Timestamp,
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

pub const CONTRACT_NAME: &str = "crates.io:token-vault";
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
