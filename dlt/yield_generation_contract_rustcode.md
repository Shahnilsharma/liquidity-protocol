# Rust Source Files Documentation

## src/bin/schema.rs

```rust
use cosmwasm_schema::write_api;

use simple_lending_borrowing::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        execute: ExecuteMsg,
        query: QueryMsg,
    }
}
```

## src/contract.rs

```rust
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, BankMsg, Binary, Coin, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128,
};
use cw2::set_contract_version;
use cw_utils::must_pay;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, PoolResponse, QueryMsg, UserResponse};
use crate::state::{
    BorrowerInfo, Config, LenderInfo, Pool, BORROWERS, CONFIG, LENDERS, POOL,
};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:simple-lending-borrowing";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let config = Config {
        denom: msg.denom,
        interest_rate: msg.interest_rate,
    };
    CONFIG.save(deps.storage, &config)?;

    let pool = Pool {
        total_lent: Uint128::zero(),
        total_borrowed: Uint128::zero(),
        total_shares: Uint128::zero(),
    };
    POOL.save(deps.storage, &pool)?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new().add_attribute("action", "instantiate"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Deposit {} => execute_deposit(deps, info),
        ExecuteMsg::Withdraw { amount } => execute_withdraw(deps, info, amount),
        ExecuteMsg::Borrow { amount } => execute_borrow(deps, info, amount),
        ExecuteMsg::Repay {} => execute_repay(deps, info),
    }
}

pub fn execute_deposit(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let amount = must_pay(&info, &config.denom).map_err(|_| ContractError::InvalidFunds {})?;

    let mut pool = POOL.load(deps.storage)?;
    
    let shares = if pool.total_shares.is_zero() {
        amount
    } else {
        amount.multiply_ratio(pool.total_shares, pool.total_lent)
    };

    pool.total_lent += amount;
    pool.total_shares += shares;
    POOL.save(deps.storage, &pool)?;

    LENDERS.update(deps.storage, &info.sender, |old| -> StdResult<_> {
        let mut info = old.unwrap_or(LenderInfo { shares: Uint128::zero() });
        info.shares += shares;
        Ok(info)
    })?;

    Ok(Response::new()
        .add_attribute("action", "deposit")
        .add_attribute("amount", amount)
        .add_attribute("shares", shares))
}

pub fn execute_withdraw(
    deps: DepsMut,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let mut pool = POOL.load(deps.storage)?;
    let mut lender = LENDERS.load(deps.storage, &info.sender)?;

    // Check liquidity
    let available = pool.total_lent.checked_sub(pool.total_borrowed).map_err(|_| ContractError::InsufficientLiquidity {})?;
    if amount > available {
        return Err(ContractError::InsufficientLiquidity {});
    }

    // Calculate shares to burn
    let shares_to_burn = amount.multiply_ratio(pool.total_shares, pool.total_lent);
    
    if lender.shares < shares_to_burn {
        return Err(ContractError::InsufficientFunds {});
    }

    lender.shares -= shares_to_burn;
    pool.total_shares -= shares_to_burn;
    pool.total_lent -= amount;

    LENDERS.save(deps.storage, &info.sender, &lender)?;
    POOL.save(deps.storage, &pool)?;

    let bank_msg = BankMsg::Send {
        to_address: info.sender.to_string(),
        amount: vec![Coin {
            denom: config.denom,
            amount,
        }],
    };

    Ok(Response::new()
        .add_message(bank_msg)
        .add_attribute("action", "withdraw")
        .add_attribute("amount", amount))
}

pub fn execute_borrow(
    deps: DepsMut,
    _info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let mut pool = POOL.load(deps.storage)?;

    // Check if there is enough liquidity (total_lent - total_borrowed)
    let available = pool.total_lent.checked_sub(pool.total_borrowed).map_err(|_| ContractError::InsufficientLiquidity {})?;
    if amount > available {
        return Err(ContractError::InsufficientLiquidity {});
    }

    pool.total_borrowed += amount;
    POOL.save(deps.storage, &pool)?;

    BORROWERS.update(deps.storage, &_info.sender, |old| -> StdResult<_> {
        let mut info = old.unwrap_or(BorrowerInfo { amount: Uint128::zero() });
        info.amount += amount;
        Ok(info)
    })?;

    let bank_msg = BankMsg::Send {
        to_address: _info.sender.to_string(),
        amount: vec![Coin {
            denom: config.denom,
            amount,
        }],
    };

    Ok(Response::new()
        .add_message(bank_msg)
        .add_attribute("action", "borrow")
        .add_attribute("amount", amount))
}

pub fn execute_repay(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let amount_paid = must_pay(&info, &config.denom).map_err(|_| ContractError::InvalidFunds {})?;

    let borrower = BORROWERS.load(deps.storage, &info.sender).map_err(|_| ContractError::NoDebt {})?;
    let mut pool = POOL.load(deps.storage)?;

    // Simple interest calculation: interest = amount * rate / 1_000_000
    let interest = borrower.amount.multiply_ratio(config.interest_rate, Uint128::new(1_000_000));
    let total_due = borrower.amount + interest;

    if amount_paid < total_due {
        // Partial repayment? For simplicity, requires full repayment of principal + interest
        // or just apply to principal first.
        // Let's just say they must pay at least interest + some principal.
        return Err(ContractError::InsufficientFunds {});
    }

    // Update pool
    pool.total_borrowed -= borrower.amount;
    pool.total_lent += interest; // Interest increases the value of the pool (yield)
    
    POOL.save(deps.storage, &pool)?;
    BORROWERS.remove(deps.storage, &info.sender);

    // If they overpaid, send back the change
    let change = amount_paid.checked_sub(total_due).unwrap_or_default();
    let mut res = Response::new()
        .add_attribute("action", "repay")
        .add_attribute("principal", borrower.amount)
        .add_attribute("interest", interest);

    if !change.is_zero() {
        res = res.add_message(BankMsg::Send {
            to_address: info.sender.to_string(),
            amount: vec![Coin {
                denom: config.denom,
                amount: change,
            }],
        });
    }

    Ok(res)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetPool {} => to_json_binary(&query_pool(deps)?),
        QueryMsg::GetUser { address } => to_json_binary(&query_user(deps, address)?),
    }
}

fn query_pool(deps: Deps) -> StdResult<PoolResponse> {
    let pool = POOL.load(deps.storage)?;
    Ok(PoolResponse {
        total_lent: pool.total_lent,
        total_borrowed: pool.total_borrowed,
    })
}

fn query_user(deps: Deps, address: String) -> StdResult<UserResponse> {
    let addr = deps.api.addr_validate(&address)?;
    let lender = LENDERS.may_load(deps.storage, &addr)?.unwrap_or(LenderInfo { shares: Uint128::zero() });
    let borrower = BORROWERS.may_load(deps.storage, &addr)?.unwrap_or(BorrowerInfo { amount: Uint128::zero() });
    
    let pool = POOL.load(deps.storage)?;
    
    // Convert shares back to amount
    let lent_amount = if pool.total_shares.is_zero() {
        Uint128::zero()
    } else {
        lender.shares.multiply_ratio(pool.total_lent, pool.total_shares)
    };

    Ok(UserResponse {
        lent: lent_amount,
        borrowed: borrower.amount,
    })
}

#[cfg(test)]
mod tests {}
```

## src/error.rs

```rust
use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Insufficient funds")]
    InsufficientFunds {},

    #[error("Invalid funds")]
    InvalidFunds {},

    #[error("No debt to repay")]
    NoDebt {},

    #[error("Insufficient liquidity in pool")]
    InsufficientLiquidity {},
}
```

## src/helpers.rs

```rust
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{to_json_binary, Addr, CosmosMsg, StdResult, WasmMsg};

use crate::msg::ExecuteMsg;

/// CwTemplateContract is a wrapper around Addr that provides a lot of helpers
/// for working with this.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct CwTemplateContract(pub Addr);

impl CwTemplateContract {
    pub fn addr(&self) -> Addr {
        self.0.clone()
    }

    pub fn call<T: Into<ExecuteMsg>>(&self, msg: T) -> StdResult<CosmosMsg> {
        let msg = to_json_binary(&msg.into())?;
        Ok(WasmMsg::Execute {
            contract_addr: self.addr().into(),
            msg,
            funds: vec![],
        }
        .into())
    }
}
```

## src/lib.rs

```rust
pub mod contract;
mod error;
pub mod helpers;
pub mod msg;
pub mod state;

pub use crate::error::ContractError;
```

## src/msg.rs

```rust
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;

#[cw_serde]
pub struct InstantiateMsg {
    pub denom: String,
    pub interest_rate: Uint128,
}

#[cw_serde]
pub enum ExecuteMsg {
    Deposit {},
    Withdraw { amount: Uint128 },
    Borrow { amount: Uint128 },
    Repay {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(PoolResponse)]
    GetPool {},
    #[returns(UserResponse)]
    GetUser { address: String },
}

#[cw_serde]
pub struct PoolResponse {
    pub total_lent: Uint128,
    pub total_borrowed: Uint128,
}

#[cw_serde]
pub struct UserResponse {
    pub lent: Uint128,
    pub borrowed: Uint128,
}
```

## src/state.rs

```rust
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};

#[cw_serde]
pub struct Config {
    pub denom: String,
    pub interest_rate: Uint128, // Scaled by 1e6 (e.g., 50000 = 5%)
}

#[cw_serde]
pub struct Pool {
    pub total_lent: Uint128,      // Total assets in pool (available + out)
    pub total_borrowed: Uint128,  // Assets currently out
    pub total_shares: Uint128,    // Total shares issued to lenders
}

#[cw_serde]
pub struct LenderInfo {
    pub shares: Uint128,
}

#[cw_serde]
pub struct BorrowerInfo {
    pub amount: Uint128,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const POOL: Item<Pool> = Item::new("pool");
pub const LENDERS: Map<&Addr, LenderInfo> = Map::new("lenders");
pub const BORROWERS: Map<&Addr, BorrowerInfo> = Map::new("borrowers");
```

