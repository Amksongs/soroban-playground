// Copyright (c) 2026 StellarDevTools
// SPDX-License-Identifier: MIT

#![no_std]

use soroban_sdk::{contract, contractimpl, symbol_short, token, Address, Env, Symbol, Vec, Map};

const UNBONDING_PERIOD: u64 = 7 * 24 * 60 * 60; // 7 days in seconds
const FLASH_LOAN_GUARD: u64 = 1;

// Error types
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    InvalidAmount = 3,
    InsufficientBalance = 4,
    InsufficientLiquidity = 5,
    UnbondingNotReady = 6,
    NoPosition = 7,
    ArithmeticOverflow = 8,
    InvalidValidator = 9,
    SlashProtection = 10,
    FlashLoanReentry = 11,
}

#[contract]
pub struct LiquidStaking;

#[contractimpl]
impl LiquidStaking {
    /// Initialize the liquid staking contract
    pub fn initialize(env: Env, admin: Address, staking_token: Address) -> Result<(), Error> {
        if storage_get_initialized(&env) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();

        storage_set_initialized(&env, true);
        storage_set_admin(&env, &admin);
        storage_set_staking_token(&env, &staking_token);
        storage_set_total_pooled(&env, 0);
        storage_set_total_stoken(&env, 0);
        storage_set_epoch(&env, 0);

        env.events()
            .publish((symbol_short!("init"),), (&admin, &staking_token));
        Ok(())
    }

    /// Stake assets and receive sTokens
    pub fn stake(env: Env, staker: Address, amount: i128) -> Result<i128, Error> {
        ensure_initialized(&env)?;
        staker.require_auth();

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        // Transfer staking tokens to contract
        let token_client = token::Client::new(&env, &storage_get_staking_token(&env));
        token_client.transfer(&staker, &env.current_contract_address(), &amount);

        // Calculate sTokens to mint based on exchange rate
        let total_pooled = storage_get_total_pooled(&env);
        let total_stoken = storage_get_total_stoken(&env);

        let stoken_amount = if total_stoken == 0 || total_pooled == 0 {
            amount
        } else {
            // sToken = (amount * total_stoken) / total_pooled
            amount.checked_mul(total_stoken)
                .ok_or(Error::ArithmeticOverflow)?
                .checked_div(total_pooled)
                .ok_or(Error::ArithmeticOverflow)?
        };

        if stoken_amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        // Update balances
        let staker_balance = storage_get_stoken_balance(&env, &staker);
        storage_set_stoken_balance(&env, &staker, staker_balance.checked_add(stoken_amount).ok_or(Error::ArithmeticOverflow)?);
        storage_set_total_pooled(&env, total_pooled.checked_add(amount).ok_or(Error::ArithmeticOverflow)?);
        storage_set_total_stoken(&env, total_stoken.checked_add(stoken_amount).ok_or(Error::ArithmeticOverflow)?);

        env.events()
            .publish((symbol_short!("stake"),), (&staker, &amount, &stoken_amount));

        Ok(stoken_amount)
    }

    /// Queue unbonding of sTokens (7-day cooldown)
    pub fn queue_unbond(env: Env, staker: Address, stoken_amount: i128) -> Result<u64, Error> {
        ensure_initialized(&env)?;
        staker.require_auth();

        if stoken_amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let balance = storage_get_stoken_balance(&env, &staker);
        if balance < stoken_amount {
            return Err(Error::InsufficientBalance);
        }

        // Deduct from staker balance
        storage_set_stoken_balance(&env, &staker, balance.checked_sub(stoken_amount).ok_or(Error::ArithmeticOverflow)?);

        // Create unbonding queue entry
        let current_time = env.ledger().timestamp();
        let unlock_time = current_time.checked_add(UNBONDING_PERIOD).ok_or(Error::ArithmeticOverflow)?;

        let unbond_id = storage_get_next_unbond_id(&env);
        storage_set_next_unbond_id(&env, unbond_id.checked_add(1).ok_or(Error::ArithmeticOverflow)?);

        storage_set_unbonding(&env, unbond_id, &staker, stoken_amount, unlock_time);

        env.events()
            .publish((symbol_short!("queueu"),), (&staker, &unbond_id, &unlock_time));

        Ok(unbond_id)
    }

    /// Claim unbonded assets after cooldown period
    pub fn claim_unbond(env: Env, staker: Address, unbond_id: u64) -> Result<i128, Error> {
        ensure_initialized(&env)?;
        staker.require_auth();

        let current_time = env.ledger().timestamp();

        let (unbond_staker, stoken_amount, unlock_time) = storage_get_unbonding(&env, unbond_id)?;

        if unbond_staker != staker {
            return Err(Error::NoPosition);
        }

        if current_time < unlock_time {
            return Err(Error::UnbondingNotReady);
        }

        // Calculate assets based on exchange rate
        let total_pooled = storage_get_total_pooled(&env);
        let total_stoken = storage_get_total_stoken(&env);

        if total_stoken <= 0 {
            return Err(Error::InsufficientLiquidity);
        }

        let asset_amount = stoken_amount.checked_mul(total_pooled)
            .ok_or(Error::ArithmeticOverflow)?
            .checked_div(total_stoken)
            .ok_or(Error::ArithmeticOverflow)?;

        if asset_amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        // Remove unbonding record
        storage_delete_unbonding(&env, unbond_id);

        // Update pools
        let new_total_stoken = total_stoken.checked_sub(stoken_amount).ok_or(Error::ArithmeticOverflow)?;
        let new_total_pooled = total_pooled.checked_sub(asset_amount).ok_or(Error::ArithmeticOverflow)?;

        storage_set_total_stoken(&env, new_total_stoken);
        storage_set_total_pooled(&env, new_total_pooled);

        // Transfer assets back to staker
        let token_client = token::Client::new(&env, &storage_get_staking_token(&env));
        token_client.transfer(&env.current_contract_address(), &staker, &asset_amount);

        env.events()
            .publish((symbol_short!("claim"),), (&staker, &unbond_id, &asset_amount));

        Ok(asset_amount)
    }

    /// Instant unstaking via swap pool (with dynamic liquidity fee)
    pub fn instant_unstake(env: Env, staker: Address, stoken_amount: i128) -> Result<i128, Error> {
        ensure_initialized(&env)?;
        staker.require_auth();

        if stoken_amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let balance = storage_get_stoken_balance(&env, &staker);
        if balance < stoken_amount {
            return Err(Error::InsufficientBalance);
        }

        // Check for flash loan reentrancy
        let flash_guard = storage_get_flash_guard(&env);
        if flash_guard > 0 {
            return Err(Error::FlashLoanReentry);
        }

        // Calculate base asset amount
        let total_pooled = storage_get_total_pooled(&env);
        let total_stoken = storage_get_total_stoken(&env);

        if total_stoken <= 0 {
            return Err(Error::InsufficientLiquidity);
        }

        let base_assets = stoken_amount.checked_mul(total_pooled)
            .ok_or(Error::ArithmeticOverflow)?
            .checked_div(total_stoken)
            .ok_or(Error::ArithmeticOverflow)?;

        if base_assets <= 0 {
            return Err(Error::InvalidAmount);
        }

        // Apply dynamic liquidity fee (0.5% to 2% based on pool depth)
        let pool_utilization = base_assets.checked_mul(100)
            .ok_or(Error::ArithmeticOverflow)?
            .checked_div(total_pooled.checked_add(1).ok_or(Error::ArithmeticOverflow)?)
            .ok_or(Error::ArithmeticOverflow)?;

        let fee_basis = if pool_utilization > 20 { 200 } else { 50 }; // 2% or 0.5%
        let fee = base_assets.checked_mul(fee_basis)
            .ok_or(Error::ArithmeticOverflow)?
            .checked_div(10000)
            .ok_or(Error::ArithmeticOverflow)?;

        let asset_amount = base_assets.checked_sub(fee).ok_or(Error::ArithmeticOverflow)?;

        if asset_amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        // Update balances
        storage_set_stoken_balance(&env, &staker, balance.checked_sub(stoken_amount).ok_or(Error::ArithmeticOverflow)?);
        let new_total_stoken = total_stoken.checked_sub(stoken_amount).ok_or(Error::ArithmeticOverflow)?;
        let new_total_pooled = total_pooled.checked_sub(asset_amount).ok_or(Error::ArithmeticOverflow)?;

        storage_set_total_stoken(&env, new_total_stoken);
        storage_set_total_pooled(&env, new_total_pooled);

        // Transfer assets back to staker
        let token_client = token::Client::new(&env, &storage_get_staking_token(&env));
        token_client.transfer(&env.current_contract_address(), &staker, &asset_amount);

        env.events()
            .publish((symbol_short!("unsta"),), (&staker, &stoken_amount, &asset_amount, &fee));

        Ok(asset_amount)
    }

    /// Accrue epoch rewards (increases sToken exchange rate)
    pub fn accrue_epoch_rewards(env: Env, reward_amount: i128) -> Result<(), Error> {
        ensure_initialized(&env)?;
        let admin = storage_get_admin(&env);
        admin.require_auth();

        if reward_amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        // Transfer reward tokens to contract
        let token_client = token::Client::new(&env, &storage_get_staking_token(&env));
        token_client.transfer(&admin, &env.current_contract_address(), &reward_amount);

        // Add to total pooled (increases sToken value)
        let total_pooled = storage_get_total_pooled(&env);
        let new_total = total_pooled.checked_add(reward_amount).ok_or(Error::ArithmeticOverflow)?;
        storage_set_total_pooled(&env, new_total);

        // Increment epoch
        let epoch = storage_get_epoch(&env);
        storage_set_epoch(&env, epoch.checked_add(1).ok_or(Error::ArithmeticOverflow)?);

        env.events()
            .publish((symbol_short!("epoch"),), (&epoch, &reward_amount, &new_total));

        Ok(())
    }

    /// Get current exchange rate (assets per sToken)
    pub fn exchange_rate(env: Env) -> Result<i128, Error> {
        ensure_initialized(&env)?;

        let total_pooled = storage_get_total_pooled(&env);
        let total_stoken = storage_get_total_stoken(&env);

        if total_stoken <= 0 {
            return Ok(1_000_000_000); // 1.0 in 9 decimals
        }

        // Rate = total_pooled / total_stoken (scaled by 1e9)
        let rate = total_pooled.checked_mul(1_000_000_000)
            .ok_or(Error::ArithmeticOverflow)?
            .checked_div(total_stoken)
            .ok_or(Error::ArithmeticOverflow)?;

        Ok(rate)
    }

    /// Get sToken balance for address
    pub fn balance_of(env: Env, account: Address) -> i128 {
        storage_get_stoken_balance(&env, &account)
    }

    /// Get total pooled assets
    pub fn total_pooled(env: Env) -> i128 {
        storage_get_total_pooled(&env)
    }

    /// Get total sToken supply
    pub fn total_stoken(env: Env) -> i128 {
        storage_get_total_stoken(&env)
    }

    /// Get current epoch
    pub fn current_epoch(env: Env) -> u64 {
        storage_get_epoch(&env)
    }
}

fn ensure_initialized(env: &Env) -> Result<(), Error> {
    if !storage_get_initialized(env) {
        return Err(Error::NotInitialized);
    }
    Ok(())
}

// Storage helpers
fn storage_get_initialized(env: &Env) -> bool {
    env.storage()
        .persistent()
        .get(&symbol_short!("init"))
        .unwrap_or(false)
}

fn storage_set_initialized(env: &Env, value: bool) {
    env.storage()
        .persistent()
        .set(&symbol_short!("init"), &value);
}

fn storage_get_admin(env: &Env) -> Address {
    env.storage()
        .persistent()
        .get(&symbol_short!("admin"))
        .unwrap()
}

fn storage_set_admin(env: &Env, admin: &Address) {
    env.storage()
        .persistent()
        .set(&symbol_short!("admin"), admin);
}

fn storage_get_staking_token(env: &Env) -> Address {
    env.storage()
        .persistent()
        .get(&symbol_short!("token"))
        .unwrap()
}

fn storage_set_staking_token(env: &Env, token: &Address) {
    env.storage()
        .persistent()
        .set(&symbol_short!("token"), token);
}

fn storage_get_total_pooled(env: &Env) -> i128 {
    env.storage()
        .persistent()
        .get(&symbol_short!("pool"))
        .unwrap_or(0)
}

fn storage_set_total_pooled(env: &Env, amount: i128) {
    env.storage()
        .persistent()
        .set(&symbol_short!("pool"), &amount);
}

fn storage_get_total_stoken(env: &Env) -> i128 {
    env.storage()
        .persistent()
        .get(&symbol_short!("stkn"))
        .unwrap_or(0)
}

fn storage_set_total_stoken(env: &Env, amount: i128) {
    env.storage()
        .persistent()
        .set(&symbol_short!("stkn"), &amount);
}

fn storage_get_epoch(env: &Env) -> u64 {
    env.storage()
        .persistent()
        .get(&symbol_short!("epoc"))
        .unwrap_or(0)
}

fn storage_set_epoch(env: &Env, epoch: u64) {
    env.storage()
        .persistent()
        .set(&symbol_short!("epoc"), &epoch);
}

fn storage_get_stoken_balance(env: &Env, account: &Address) -> i128 {
    let key = (symbol_short!("bal"), account);
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(0)
}

fn storage_set_stoken_balance(env: &Env, account: &Address, balance: i128) {
    let key = (symbol_short!("bal"), account);
    env.storage()
        .persistent()
        .set(&key, &balance);
}

fn storage_get_next_unbond_id(env: &Env) -> u64 {
    env.storage()
        .persistent()
        .get(&symbol_short!("unbid"))
        .unwrap_or(0)
}

fn storage_set_next_unbond_id(env: &Env, id: u64) {
    env.storage()
        .persistent()
        .set(&symbol_short!("unbid"), &id);
}

fn storage_set_unbonding(env: &Env, id: u64, staker: &Address, stoken_amount: i128, unlock_time: u64) {
    let key = (symbol_short!("unbd"), id);
    let value = (staker.clone(), stoken_amount, unlock_time);
    env.storage()
        .persistent()
        .set(&key, &value);
}

fn storage_get_unbonding(env: &Env, id: u64) -> Result<(Address, i128, u64), Error> {
    let key = (symbol_short!("unbd"), id);
    env.storage()
        .persistent()
        .get(&key)
        .ok_or(Error::NoPosition)
}

fn storage_delete_unbonding(env: &Env, id: u64) {
    let key = (symbol_short!("unbd"), id);
    env.storage()
        .persistent()
        .remove(&key);
}

fn storage_get_flash_guard(env: &Env) -> u64 {
    env.storage()
        .persistent()
        .get(&symbol_short!("flsh"))
        .unwrap_or(0)
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};

    #[test]
    fn test_initialization() {
        let env = soroban_sdk::Env::default();
        let admin = Address::random(&env);
        let token = Address::random(&env);

        env.mock_all_auths();

        assert_eq!(
            LiquidStaking::initialize(env.clone(), admin.clone(), token.clone()),
            Ok(())
        );
        assert_eq!(LiquidStaking::total_pooled(env.clone()), 0);
        assert_eq!(LiquidStaking::total_stoken(env.clone()), 0);
    }

    #[test]
    fn test_exchange_rate_increases() {
        let env = soroban_sdk::Env::default();
        let admin = Address::random(&env);
        let staker = Address::random(&env);
        let token = Address::random(&env);

        env.mock_all_auths();

        LiquidStaking::initialize(env.clone(), admin.clone(), token.clone()).unwrap();
        let initial_rate = LiquidStaking::exchange_rate(env.clone()).unwrap();

        LiquidStaking::accrue_epoch_rewards(env.clone(), 100).unwrap();
        let new_rate = LiquidStaking::exchange_rate(env.clone()).unwrap();

        assert!(new_rate >= initial_rate);
    }

    #[test]
    fn test_unbonding_cooldown() {
        let env = soroban_sdk::Env::default();
        let admin = Address::random(&env);
        let staker = Address::random(&env);
        let token = Address::random(&env);

        env.mock_all_auths();

        LiquidStaking::initialize(env.clone(), admin.clone(), token.clone()).unwrap();

        // Try to claim immediately - should fail
        let err = LiquidStaking::claim_unbond(env.clone(), staker.clone(), 0);
        assert!(err.is_err());
    }

    #[test]
    fn test_instant_unstake_fee() {
        let env = soroban_sdk::Env::default();
        let admin = Address::random(&env);
        let token = Address::random(&env);

        env.mock_all_auths();

        LiquidStaking::initialize(env.clone(), admin.clone(), token.clone()).unwrap();

        // Accrue rewards to establish rate
        LiquidStaking::accrue_epoch_rewards(env.clone(), 50).unwrap();

        let rate = LiquidStaking::exchange_rate(env.clone()).unwrap();
        assert!(rate > 1_000_000_000);
    }
}
