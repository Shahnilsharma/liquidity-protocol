# Rust Source Code Collection

## src/contract.rs
```rust
use cosmwasm_std::{
    entry_point, to_json_binary, Addr, BankMsg, Binary, Coin, Deps, DepsMut, Env, MessageInfo,
    QuerierWrapper, Response, StdResult, Uint128,
};
use cw2::set_contract_version;

use crate::custom::{burn_tokens_msg, create_denom_msg, mint_and_send_tokens_msg};
use crate::error::ContractError;
use crate::msg::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, VaultInfoResponse, QueryMsg,
    UserInfoResponse,
};
use crate::state::{Config, VaultState, CONFIG, CONTRACT_NAME, CONTRACT_VERSION, VAULT_STATE};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    // Validate subdenom requirements (3-44 chars, start with lowercase)
    if msg.lp_subdenom.len() < 3 || msg.lp_subdenom.len() > 44 {
        return Err(ContractError::InvalidSubdenom {
            reason: "Subdenom must be 3-44 characters".to_string(),
        });
    }

    if !msg
        .lp_subdenom
        .chars()
        .next()
        .unwrap()
        .is_ascii_lowercase()
    {
        return Err(ContractError::InvalidSubdenom {
            reason: "Subdenom must start with lowercase letter".to_string(),
        });
    }

    // Validate minting cap (must be > 0 as per ZigChain requirements)
    if msg.lp_minting_cap.is_zero() {
        return Err(ContractError::InvalidMintingCap {});
    }

    let admin_addr = match msg.admin {
        Some(addr) => deps.api.addr_validate(&addr)?,
        None => info.sender.clone(),
    };

    // Construct LP token full denom
    let lp_full_denom = format!("coin.{}.{}", env.contract.address, msg.lp_subdenom);

    // Initialize configuration
    let config = Config {
        stablecoin_denom: msg.stablecoin_denom.clone(),
        lp_full_denom: lp_full_denom.clone(),
        admin: admin_addr.clone(),
    };
    CONFIG.save(deps.storage, &config)?;

    // Initialize vault state
    let vault_state = VaultState {
        total_stablecoin_deposited: Uint128::zero(),
        total_lp_minted: Uint128::zero(),
    };
    VAULT_STATE.save(deps.storage, &vault_state)?;

    // Set contract version
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // Create LP token denom via TokenFactory
    let create_denom = create_denom_msg(
        env.contract.address.to_string(),
        msg.lp_subdenom.clone(),
        msg.lp_minting_cap.to_string(),
        msg.can_change_minting_cap.unwrap_or(true),
        msg.uri,
        msg.uri_hash,
        msg.description,
    )?;

    Ok(Response::new()
        .add_message(create_denom)
        .add_attribute("method", "instantiate")
        .add_attribute("stablecoin_denom", msg.stablecoin_denom)
        .add_attribute("lp_full_denom", lp_full_denom)
        .add_attribute("lp_minting_cap", msg.lp_minting_cap)
        .add_attribute("admin", admin_addr))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Deposit {} => execute_deposit(deps, env, info),
        ExecuteMsg::Withdraw {} => execute_withdraw(deps, env, info),
        ExecuteMsg::UpdateConfig {
            stablecoin_denom,
            admin,
        } => execute_update_config(deps, info, stablecoin_denom, admin),
    }
}

fn execute_deposit(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Find and validate the stablecoin sent
    let stablecoin_sent = info
        .funds
        .iter()
        .find(|coin| coin.denom == config.stablecoin_denom)
        .ok_or(ContractError::NoStablecoinSent {})?;

    let amount = stablecoin_sent.amount;
    if amount.is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    // For 1:1 exchange, LP amount equals deposit amount
    let lp_amount = amount;

    // Update vault state
    let mut vault_state = VAULT_STATE.load(deps.storage)?;
    vault_state.total_stablecoin_deposited = vault_state
        .total_stablecoin_deposited
        .checked_add(amount)
        .map_err(|_| ContractError::OverflowError {
            operation: "deposit".to_string(),
        })?;

    vault_state.total_lp_minted = vault_state
        .total_lp_minted
        .checked_add(lp_amount)
        .map_err(|_| ContractError::OverflowError {
            operation: "LP mint".to_string(),
        })?;

    VAULT_STATE.save(deps.storage, &vault_state)?;

    // Mint LP tokens to the user via TokenFactory
    let mint_msg = mint_and_send_tokens_msg(
        env.contract.address.to_string(),
        config.lp_full_denom.clone(),
        lp_amount.to_string(),
        info.sender.to_string(),
    )?;

    Ok(Response::new()
        .add_message(mint_msg)
        .add_attribute("method", "deposit")
        .add_attribute("user", info.sender)
        .add_attribute("stablecoin_amount", amount)
        .add_attribute("lp_amount", lp_amount))
}

fn execute_withdraw(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Find and validate the LP tokens sent
    let lp_sent = info
        .funds
        .iter()
        .find(|coin| coin.denom == config.lp_full_denom)
        .ok_or(ContractError::NoLpTokensSent {})?;

    let lp_amount = lp_sent.amount;
    if lp_amount.is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    // For 1:1 exchange, stablecoin amount equals LP amount
    let stablecoin_amount = lp_amount;

    // Update vault state
    let mut vault_state = VAULT_STATE.load(deps.storage)?;

    if vault_state.total_stablecoin_deposited < stablecoin_amount {
        return Err(ContractError::InsufficientVaultBalance {});
    }

    vault_state.total_stablecoin_deposited = vault_state
        .total_stablecoin_deposited
        .checked_sub(stablecoin_amount)
        .map_err(|_| ContractError::OverflowError {
            operation: "withdrawal".to_string(),
        })?;

    vault_state.total_lp_minted = vault_state
        .total_lp_minted
        .checked_sub(lp_amount)
        .map_err(|_| ContractError::OverflowError {
            operation: "LP burn".to_string(),
        })?;

    VAULT_STATE.save(deps.storage, &vault_state)?;

    // Burn LP tokens via TokenFactory
    let burn_msg = burn_tokens_msg(
        env.contract.address.to_string(),
        config.lp_full_denom.clone(),
        lp_amount.to_string(),
    )?;

    // Return stablecoin to user via Bank module
    let return_msg = BankMsg::Send {
        to_address: info.sender.to_string(),
        amount: vec![Coin {
            denom: config.stablecoin_denom.clone(),
            amount: stablecoin_amount,
        }],
    };

    Ok(Response::new()
        .add_message(burn_msg)
        .add_message(return_msg)
        .add_attribute("method", "withdraw")
        .add_attribute("user", info.sender)
        .add_attribute("lp_amount", lp_amount)
        .add_attribute("stablecoin_amount", stablecoin_amount))
}

fn execute_update_config(
    deps: DepsMut,
    info: MessageInfo,
    stablecoin_denom: Option<String>,
    admin: Option<String>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    if info.sender != config.admin {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(denom) = stablecoin_denom {
        config.stablecoin_denom = denom;
    }

    if let Some(addr) = admin {
        config.admin = deps.api.addr_validate(&addr)?;
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_attribute("method", "update_config")
        .add_attribute("admin", info.sender))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&query_config(deps)?),
        QueryMsg::VaultInfo {} => to_json_binary(&query_vault_info(deps)?),
        QueryMsg::UserInfo { address } => to_json_binary(&query_user_info(deps, address)?),
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        stablecoin_denom: config.stablecoin_denom,
        lp_full_denom: config.lp_full_denom,
        admin: config.admin,
    })
}

fn query_vault_info(deps: Deps) -> StdResult<VaultInfoResponse> {
    let vault_state = VAULT_STATE.load(deps.storage)?;

    Ok(VaultInfoResponse {
        total_stablecoin_deposited: vault_state.total_stablecoin_deposited,
        total_lp_supply: vault_state.total_lp_minted,
    })
}

fn query_user_info(deps: Deps, address: String) -> StdResult<UserInfoResponse> {
    let config = CONFIG.load(deps.storage)?;
    let user_addr = deps.api.addr_validate(&address)?;

    // Query LP balance from bank module
    let lp_balance = query_bank_balance(&deps.querier, &user_addr, &config.lp_full_denom)?;

    // For 1:1 exchange, stablecoin value equals LP balance
    let stablecoin_value = lp_balance;

    Ok(UserInfoResponse {
        address: user_addr,
        lp_balance,
        stablecoin_value,
    })
}

/// Query balance from bank module
fn query_bank_balance(
    querier: &QuerierWrapper,
    address: &Addr,
    denom: &str,
) -> StdResult<Uint128> {
    let balance = querier.query_balance(address, denom)?;
    Ok(balance.amount)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Ok(Response::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, from_json};

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000_000),
            can_change_minting_cap: Some(false),
            uri: None,
            uri_hash: None,
            description: Some("LP Token for Liquidity Pool".to_string()),
            admin: None,
        };
        let info = mock_info("creator", &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(1, res.messages.len());

        let res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
        let config: ConfigResponse = from_json(&res).unwrap();
        assert_eq!("uzig", config.stablecoin_denom);
        assert!(config.lp_full_denom.contains("lplp"));
        assert_eq!("creator", config.admin);
    }

    #[test]
    fn test_invalid_subdenom() {
        let mut deps = mock_dependencies();

        // Too short
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "ab".to_string(),
            lp_minting_cap: Uint128::new(1_000_000),
            can_change_minting_cap: None,
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
        };
        let info = mock_info("creator", &[]);
        let err = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap_err();
        assert!(matches!(err, ContractError::InvalidSubdenom { .. }));

        // Doesn't start with lowercase
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "ABC".to_string(),
            lp_minting_cap: Uint128::new(1_000_000),
            can_change_minting_cap: None,
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
        };
        let err = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert!(matches!(err, ContractError::InvalidSubdenom { .. }));
    }

    #[test]
    fn test_zero_minting_cap() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::zero(),
            can_change_minting_cap: None,
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
        };
        let info = mock_info("creator", &[]);
        let err = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert!(matches!(err, ContractError::InvalidMintingCap {}));
    }
}
```

## src/custom.rs
```rust
use cosmwasm_std::{CosmosMsg, StdResult, Binary, StdError};
use prost::Message;

/// ZigChain TokenFactory message structures using prost
/// These match the exact proto definitions from ZigChain

#[derive(Clone, PartialEq, Message)]
pub struct MsgCreateDenom {
    #[prost(string, tag = "1")]
    pub sender: String,
    #[prost(string, tag = "2")]
    pub subdenom: String,
    #[prost(string, optional, tag = "3")]
    pub minting_cap: Option<String>,
    #[prost(bool, tag = "4")]
    pub can_change_minting_cap: bool,
    #[prost(string, optional, tag = "5")]
    pub uri: Option<String>,
    #[prost(string, optional, tag = "6")]
    pub uri_hash: Option<String>,
    #[prost(string, optional, tag = "7")]
    pub description: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Coin {
    #[prost(string, tag = "1")]
    pub denom: String,
    #[prost(string, tag = "2")]
    pub amount: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct MsgMintAndSendTokens {
    #[prost(string, tag = "1")]
    pub signer: String,
    #[prost(message, optional, tag = "2")]
    pub token: Option<Coin>,
    #[prost(string, tag = "3")]
    pub recipient: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct MsgBurnTokens {
    #[prost(string, tag = "1")]
    pub signer: String,
    #[prost(message, optional, tag = "2")]
    pub token: Option<Coin>,
}

/// Create a new denom using ZigChain TokenFactory
/// Returns a Stargate CosmosMsg that can be added to Response
pub fn create_denom_msg(
    creator: String,
    subdenom: String,
    minting_cap: String,
    can_change_minting_cap: bool,
    uri: Option<String>,
    uri_hash: Option<String>,
    description: Option<String>,
) -> StdResult<CosmosMsg> {
    let msg = MsgCreateDenom {
        sender: creator,
        subdenom,
        minting_cap: Some(minting_cap),
        can_change_minting_cap,
        uri,
        uri_hash,
        description,
    };

    let mut buf = Vec::new();
    msg.encode(&mut buf)
        .map_err(|e| StdError::generic_err(format!("Failed to encode MsgCreateDenom: {}", e)))?;

    #[allow(deprecated)]
    Ok(CosmosMsg::Stargate {
        type_url: "/zigchain.factory.MsgCreateDenom".to_string(),
        value: Binary::from(buf),
    })
}

/// Mint tokens and send to recipient using TokenFactory
pub fn mint_and_send_tokens_msg(
    signer: String,
    denom: String,
    amount: String,
    recipient: String,
) -> StdResult<CosmosMsg> {
    let msg = MsgMintAndSendTokens {
        signer,
        token: Some(Coin { denom, amount }),
        recipient,
    };

    let mut buf = Vec::new();
    msg.encode(&mut buf)
        .map_err(|e| StdError::generic_err(format!("Failed to encode MsgMintAndSendTokens: {}", e)))?;

    #[allow(deprecated)]
    Ok(CosmosMsg::Stargate {
        type_url: "/zigchain.factory.MsgMintAndSendTokens".to_string(),
        value: Binary::from(buf),
    })
}

/// Burn tokens using TokenFactory
pub fn burn_tokens_msg(
    signer: String,
    denom: String,
    amount: String,
) -> StdResult<CosmosMsg> {
    let msg = MsgBurnTokens {
        signer,
        token: Some(Coin { denom, amount }),
    };

    let mut buf = Vec::new();
    msg.encode(&mut buf)
        .map_err(|e| StdError::generic_err(format!("Failed to encode MsgBurnTokens: {}", e)))?;

    #[allow(deprecated)]
    Ok(CosmosMsg::Stargate {
        type_url: "/zigchain.factory.MsgBurnTokens".to_string(),
        value: Binary::from(buf),
    })
}
```

## src/error.rs
```rust
use cosmwasm_std::StdError;
use thiserror::Error;

/// Custom errors for the token vault contract
#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized - only admin can perform this action")]
    Unauthorized {},

    #[error("Invalid zero amount - amount must be greater than zero")]
    InvalidZeroAmount {},

    #[error("Insufficient LP balance - user does not have enough LP tokens to withdraw")]
    InsufficientLpBalance {},

    #[error("Insufficient vault balance - vault does not have enough stablecoins")]
    InsufficientVaultBalance {},

    #[error("Invalid subdenom - {reason}")]
    InvalidSubdenom { reason: String },

    #[error("Invalid minting cap - must be greater than zero")]
    InvalidMintingCap {},

    #[error("No stablecoin sent - must send stablecoin via info.funds")]
    NoStablecoinSent {},

    #[error("No LP tokens sent - must send LP tokens via info.funds")]
    NoLpTokensSent {},

    #[error("Overflow error during {operation}")]
    OverflowError { operation: String },

    #[error("Invalid address - {0}")]
    InvalidAddress(String),

    #[error("Vault state inconsistent - total deposited does not match LP supply")]
    InconsistentVaultState {},
}
```

## src/lib.rs
```rust
pub mod contract;
pub mod custom;
mod error;
pub mod msg;
pub mod state;

pub use crate::error::ContractError;
```

## src/msg.rs
```rust
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128};

/// Message sent when instantiating the contract
#[cw_serde]
pub struct InstantiateMsg {
    /// Denom of the stablecoin (e.g., "uzig" or existing TokenFactory denom)
    pub stablecoin_denom: String,
    /// Subdenom for the LP token (will be created as coin.{contract}.{subdenom})
    pub lp_subdenom: String,
    /// Maximum supply cap for LP tokens
    pub lp_minting_cap: Uint128,
    /// Can the minting cap be changed later
    pub can_change_minting_cap: Option<bool>,
    /// Optional metadata URI for LP token
    pub uri: Option<String>,
    /// Optional URI hash for LP token metadata
    pub uri_hash: Option<String>,
    /// Optional description for LP token
    pub description: Option<String>,
    /// Optional admin address (defaults to sender if not provided)
    pub admin: Option<String>,
}

/// Messages that can be executed on the contract
#[cw_serde]
pub enum ExecuteMsg {
    /// Deposit stablecoins to receive LP tokens
    /// User must send stablecoins via info.funds
    Deposit {},
    /// Withdraw stablecoins by burning LP tokens
    /// User must send LP tokens via info.funds
    Withdraw {},
    /// Update contract configuration (admin only)
    UpdateConfig {
        /// New stablecoin denom (optional)
        stablecoin_denom: Option<String>,
        /// New admin address (optional)
        admin: Option<String>,
    },
}

/// Query messages
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Get contract configuration
    #[returns(ConfigResponse)]
    Config {},
    
    /// Get vault information (total deposits, LP supply, etc.)
    #[returns(VaultInfoResponse)]
    VaultInfo {},
    
    /// Get user's deposit information
    #[returns(UserInfoResponse)]
    UserInfo {
        /// User address to query
        address: String,
    },
}

/// Response for Config query
#[cw_serde]
pub struct ConfigResponse {
    /// Denom of the stablecoin
    pub stablecoin_denom: String,
    /// Full denom of the LP token
    pub lp_full_denom: String,
    /// Admin address
    pub admin: Addr,
}

/// Response for VaultInfo query
#[cw_serde]
pub struct VaultInfoResponse {
    /// Total stablecoins deposited in the vault
    pub total_stablecoin_deposited: Uint128,
    /// Total LP tokens in circulation
    pub total_lp_supply: Uint128,
}

/// Response for UserInfo query
#[cw_serde]
pub struct UserInfoResponse {
    /// User's address
    pub address: Addr,
    /// Amount of LP tokens owned by user
    pub lp_balance: Uint128,
    /// Equivalent stablecoin value
    pub stablecoin_value: Uint128,
}

/// Migration message (for future upgrades)
#[cw_serde]
pub struct MigrateMsg {}
```

## src/state.rs
```rust
use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use cw_storage_plus::Item;

/// Contract configuration stored in state
#[cw_serde]
pub struct Config {
    /// Denom of the stablecoin (e.g., "uzig" or TokenFactory denom)
    pub stablecoin_denom: String,
    /// Full denom of the LP token created by this contract
    /// Format: coin.{contract_address}.{subdenom}
    pub lp_full_denom: String,
    /// Admin address that can update config
    pub admin: Addr,
}

/// Vault statistics
#[cw_serde]
pub struct VaultState {
    /// Total amount of stablecoins deposited in the vault
    pub total_stablecoin_deposited: cosmwasm_std::Uint128,
    /// Total LP tokens minted
    pub total_lp_minted: cosmwasm_std::Uint128,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const VAULT_STATE: Item<VaultState> = Item::new("vault_state");

pub const CONTRACT_NAME: &str = "crates.io:token-vault";
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
```

