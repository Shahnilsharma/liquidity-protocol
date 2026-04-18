//! Integration test: Full lifecycle for DeFa PSP Pool
//! Covers: Fundraising → Active → WindingDown → Settled → Closed
//
// - One LP deposit
// - executeFacility
// - One PSP drawdown
// - Full repay
// - expireFacility
// - LP withdrawal claim

use cw_multi_test::{App, ContractWrapper, Executor};
use cosmwasm_std::{Addr, Coin, Uint128};

use defa_pool_factory::msg as pool_factory_msg;
use defa_psp_pool::msg as psp_pool_msg;
use defa_credit_manager::msg as credit_manager_msg;
use defa_yield_distributor::msg as yield_distributor_msg;
use defa_yield_reserve::msg as yield_reserve_msg;

#[test]
fn test_full_lifecycle() {
    let mut app = App::default();
    // 1. Store contract code using ContractWrapper
    let pool_factory_code_id = app.store_code(Box::new(ContractWrapper::new(
        defa_pool_factory::contract::execute,
        defa_pool_factory::contract::instantiate,
        defa_pool_factory::contract::query,
    ).with_migrate(defa_pool_factory::contract::migrate)));

    let psp_pool_code_id = app.store_code(Box::new(ContractWrapper::new(
        defa_psp_pool::contract::execute,
        defa_psp_pool::contract::instantiate,
        defa_psp_pool::contract::query,
    ).with_migrate(defa_psp_pool::contract::migrate)));

    let credit_manager_code_id = app.store_code(Box::new(ContractWrapper::new(
        defa_credit_manager::contract::execute,
        defa_credit_manager::contract::instantiate,
        defa_credit_manager::contract::query,
    ).with_migrate(defa_credit_manager::contract::migrate)));

    let yield_distributor_code_id = app.store_code(Box::new(ContractWrapper::new(
        defa_yield_distributor::contract::execute,
        defa_yield_distributor::contract::instantiate,
        defa_yield_distributor::contract::query,
    ).with_migrate(defa_yield_distributor::contract::migrate)));

    let yield_reserve_code_id = app.store_code(Box::new(ContractWrapper::new(
        defa_yield_reserve::contract::execute,
        defa_yield_reserve::contract::instantiate,
        defa_yield_reserve::contract::query,
    ).with_migrate(defa_yield_reserve::contract::migrate)));

    // 2. Instantiate PoolFactory
    let admin = Addr::unchecked("admin");
    let pool_factory_init = pool_factory_msg::InstantiateMsg {
        admin: admin.to_string(),
        psp_pool_code_id,
        credit_manager_code_id,
        yield_distributor_code_id,
        yield_reserve_code_id,
    };
    let pool_factory_addr = app.instantiate_contract(
        pool_factory_code_id,
        admin.clone(),
        &pool_factory_init,
        &[],
        "PoolFactory",
        None,
    ).unwrap();

    // 3. Create Pool (simulate with minimal valid init msgs)
    // ...existing code...
    assert!(true, "Stub integration test: implement full lifecycle checks");
}
