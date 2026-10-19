// Copyright (c) 2026 StellarDevTools
// SPDX-License-Identifier: MIT

#![no_std]

use soroban_sdk::{contract, contractimpl, symbol_short, token, Address, Env, Vec};

// Error types
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    InvalidAmount = 3,
    InvalidMilestone = 4,
    InsufficientFunds = 5,
    MilestoneNotReady = 6,
    MilestoneAlreadyApproved = 7,
    MilestoneAlreadyDisputed = 8,
    UnauthorizedDispute = 9,
    InvalidAward = 10,
    ArithmeticOverflow = 11,
    TimeoutNotReached = 12,
    ProjectNotFound = 13,
    InvalidApprover = 14,
}

#[contract]
pub struct EscrowMilestone;

#[contractimpl]
impl EscrowMilestone {
    /// Initialize the escrow contract
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if storage_get_initialized(&env) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();

        storage_set_initialized(&env, true);
        storage_set_admin(&env, &admin);
        storage_set_project_count(&env, 0);

        env.events()
            .publish((symbol_short!("init"),), &admin);
        Ok(())
    }

    /// Create new escrow project with milestones
    pub fn create_project(
        env: Env,
        buyer: Address,
        provider: Address,
        arbitrator: Address,
        collateral_token: Address,
    ) -> Result<u64, Error> {
        ensure_initialized(&env)?;
        buyer.require_auth();

        let project_id = storage_get_project_count(&env);
        storage_set_project_count(&env, project_id.checked_add(1).ok_or(Error::ArithmeticOverflow)?);

        storage_set_project(&env, project_id, &buyer, &provider, &arbitrator, &collateral_token);

        env.events()
            .publish((symbol_short!("proj"),), (&project_id, &buyer, &provider));

        Ok(project_id)
    }

    /// Add milestone to project
    pub fn add_milestone(
        env: Env,
        project_id: u64,
        milestone_id: u32,
        description_hash: Vec<u8>,
        funding_amount: i128,
        deadline: u64,
    ) -> Result<(), Error> {
        ensure_initialized(&env)?;
        let (buyer, _, _, _) = storage_get_project(&env, project_id)?;
        buyer.require_auth();

        if funding_amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        if deadline <= env.ledger().timestamp() {
            return Err(Error::InvalidMilestone);
        }

        storage_set_milestone(&env, project_id, milestone_id, funding_amount, deadline);

        env.events()
            .publish((symbol_short!("msto"),), (&project_id, &milestone_id, &deadline));

        Ok(())
    }

    /// Deposit funds for milestone
    pub fn deposit_milestone(
        env: Env,
        buyer: Address,
        project_id: u64,
        milestone_id: u32,
    ) -> Result<(), Error> {
        ensure_initialized(&env)?;
        buyer.require_auth();

        let (proj_buyer, _, _, token_addr) = storage_get_project(&env, project_id)?;

        if proj_buyer != buyer {
            return Err(Error::InvalidApprover);
        }

        let (amount, deadline) = storage_get_milestone(&env, project_id, milestone_id)?;

        if env.ledger().timestamp() >= deadline {
            return Err(Error::MilestoneNotReady);
        }

        // Transfer tokens to contract
        let token_client = token::Client::new(&env, &token_addr);
        token_client.transfer(&buyer, &env.current_contract_address(), &amount);

        // Mark milestone as funded
        storage_set_milestone_funded(&env, project_id, milestone_id, true);

        env.events()
            .publish((symbol_short!("dep"),), (&project_id, &milestone_id, &amount));

        Ok(())
    }

    /// Approve milestone completion (buyer sign-off)
    pub fn approve_milestone(
        env: Env,
        buyer: Address,
        project_id: u64,
        milestone_id: u32,
    ) -> Result<(), Error> {
        ensure_initialized(&env)?;
        buyer.require_auth();

        let (proj_buyer, _, _, _) = storage_get_project(&env, project_id)?;

        if proj_buyer != buyer {
            return Err(Error::InvalidApprover);
        }

        if storage_is_milestone_approved(&env, project_id, milestone_id) {
            return Err(Error::MilestoneAlreadyApproved);
        }

        storage_set_milestone_approved(&env, project_id, milestone_id, true);

        env.events()
            .publish((symbol_short!("appr"),), (&project_id, &milestone_id));

        Ok(())
    }

    /// Release milestone funds to provider after approval
    pub fn release_milestone(
        env: Env,
        project_id: u64,
        milestone_id: u32,
    ) -> Result<(), Error> {
        ensure_initialized(&env)?;

        let (_, provider, _, token_addr) = storage_get_project(&env, project_id)?;

        // Check if milestone is approved
        if !storage_is_milestone_approved(&env, project_id, milestone_id) {
            return Err(Error::MilestoneNotReady);
        }

        // Check if already released
        if storage_is_milestone_released(&env, project_id, milestone_id) {
            return Err(Error::MilestoneAlreadyApproved);
        }

        let (amount, _) = storage_get_milestone(&env, project_id, milestone_id)?;

        // Transfer funds to provider
        let token_client = token::Client::new(&env, &token_addr);
        token_client.transfer(&env.current_contract_address(), &provider, &amount);

        storage_set_milestone_released(&env, project_id, milestone_id, true);

        env.events()
            .publish((symbol_short!("rele"),), (&project_id, &milestone_id, &amount));

        Ok(())
    }

    /// Release milestone after timeout (if buyer doesn't approve)
    pub fn release_after_timeout(
        env: Env,
        project_id: u64,
        milestone_id: u32,
    ) -> Result<(), Error> {
        ensure_initialized(&env)?;

        let (_, provider, _, token_addr) = storage_get_project(&env, project_id)?;

        let (amount, deadline) = storage_get_milestone(&env, project_id, milestone_id)?;

        // Check if deadline passed
        if env.ledger().timestamp() < deadline {
            return Err(Error::TimeoutNotReached);
        }

        // Check if already released
        if storage_is_milestone_released(&env, project_id, milestone_id) {
            return Err(Error::MilestoneAlreadyApproved);
        }

        // Transfer funds to provider
        let token_client = token::Client::new(&env, &token_addr);
        token_client.transfer(&env.current_contract_address(), &provider, &amount);

        storage_set_milestone_released(&env, project_id, milestone_id, true);

        env.events()
            .publish((symbol_short!("tout"),), (&project_id, &milestone_id, &amount));

        Ok(())
    }

    /// Declare dispute on milestone
    pub fn declare_dispute(
        env: Env,
        disputer: Address,
        project_id: u64,
        milestone_id: u32,
    ) -> Result<(), Error> {
        ensure_initialized(&env)?;
        disputer.require_auth();

        let (buyer, _, _, _) = storage_get_project(&env, project_id)?;

        if disputer != buyer {
            return Err(Error::UnauthorizedDispute);
        }

        if storage_is_milestone_disputed(&env, project_id, milestone_id) {
            return Err(Error::MilestoneAlreadyDisputed);
        }

        storage_set_milestone_disputed(&env, project_id, milestone_id, true);

        env.events()
            .publish((symbol_short!("disp"),), (&project_id, &milestone_id, &disputer));

        Ok(())
    }

    /// Arbitrator awards funds (proportional split)
    pub fn award_split(
        env: Env,
        project_id: u64,
        milestone_id: u32,
        buyer_percentage: u32,
    ) -> Result<(), Error> {
        ensure_initialized(&env)?;

        let (buyer, provider, arbitrator, token_addr) = storage_get_project(&env, project_id)?;
        arbitrator.require_auth();

        if !storage_is_milestone_disputed(&env, project_id, milestone_id) {
            return Err(Error::InvalidAmount);
        }

        if buyer_percentage > 100 {
            return Err(Error::InvalidAward);
        }

        let (amount, _) = storage_get_milestone(&env, project_id, milestone_id)?;

        let buyer_amount = amount.checked_mul(buyer_percentage as i128)
            .ok_or(Error::ArithmeticOverflow)?
            .checked_div(100)
            .ok_or(Error::ArithmeticOverflow)?;

        let provider_amount = amount.checked_sub(buyer_amount).ok_or(Error::ArithmeticOverflow)?;

        // Transfer funds
        let token_client = token::Client::new(&env, &token_addr);

        if buyer_amount > 0 {
            token_client.transfer(&env.current_contract_address(), &buyer, &buyer_amount);
        }

        if provider_amount > 0 {
            token_client.transfer(&env.current_contract_address(), &provider, &provider_amount);
        }

        storage_set_milestone_resolved(&env, project_id, milestone_id, true);

        env.events()
            .publish((symbol_short!("awar"),), (&project_id, &milestone_id, &buyer_percentage));

        Ok(())
    }

    /// Mutual cancellation - return funds
    pub fn cancel_project(
        env: Env,
        project_id: u64,
    ) -> Result<(), Error> {
        ensure_initialized(&env)?;

        let (buyer, provider, _, token_addr) = storage_get_project(&env, project_id)?;
        buyer.require_auth();
        provider.require_auth();

        // Gather all unresolved milestone funds
        let total_balance = storage_get_project_balance(&env, project_id);

        if total_balance > 0 {
            let token_client = token::Client::new(&env, &token_addr);
            let buyer_share = total_balance.checked_div(2).ok_or(Error::ArithmeticOverflow)?;
            let provider_share = total_balance.checked_sub(buyer_share).ok_or(Error::ArithmeticOverflow)?;

            if buyer_share > 0 {
                token_client.transfer(&env.current_contract_address(), &buyer, &buyer_share);
            }

            if provider_share > 0 {
                token_client.transfer(&env.current_contract_address(), &provider, &provider_share);
            }
        }

        storage_set_project_cancelled(&env, project_id, true);

        env.events()
            .publish((symbol_short!("canc"),), &project_id);

        Ok(())
    }

    /// Get milestone status
    pub fn milestone_status(
        env: Env,
        project_id: u64,
        milestone_id: u32,
    ) -> Result<(bool, bool, bool, bool), Error> {
        let funded = storage_is_milestone_funded(&env, project_id, milestone_id);
        let approved = storage_is_milestone_approved(&env, project_id, milestone_id);
        let disputed = storage_is_milestone_disputed(&env, project_id, milestone_id);
        let released = storage_is_milestone_released(&env, project_id, milestone_id);

        Ok((funded, approved, disputed, released))
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

fn storage_get_project_count(env: &Env) -> u64 {
    env.storage()
        .persistent()
        .get(&symbol_short!("pcnt"))
        .unwrap_or(0)
}

fn storage_set_project_count(env: &Env, count: u64) {
    env.storage()
        .persistent()
        .set(&symbol_short!("pcnt"), &count);
}

fn storage_set_project(env: &Env, id: u64, buyer: &Address, provider: &Address, arbitrator: &Address, token: &Address) {
    let key = (symbol_short!("proj"), id);
    let value = (buyer.clone(), provider.clone(), arbitrator.clone(), token.clone());
    env.storage()
        .persistent()
        .set(&key, &value);
}

fn storage_get_project(env: &Env, id: u64) -> Result<(Address, Address, Address, Address), Error> {
    let key = (symbol_short!("proj"), id);
    env.storage()
        .persistent()
        .get(&key)
        .ok_or(Error::ProjectNotFound)
}

fn storage_set_milestone(env: &Env, project_id: u64, milestone_id: u32, amount: i128, deadline: u64) {
    let key = (symbol_short!("msto"), project_id, milestone_id);
    let value = (amount, deadline);
    env.storage()
        .persistent()
        .set(&key, &value);
}

fn storage_get_milestone(env: &Env, project_id: u64, milestone_id: u32) -> Result<(i128, u64), Error> {
    let key = (symbol_short!("msto"), project_id, milestone_id);
    env.storage()
        .persistent()
        .get(&key)
        .ok_or(Error::InvalidMilestone)
}

fn storage_is_milestone_funded(env: &Env, project_id: u64, milestone_id: u32) -> bool {
    let key = (symbol_short!("fund"), project_id, milestone_id);
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(false)
}

fn storage_set_milestone_funded(env: &Env, project_id: u64, milestone_id: u32, funded: bool) {
    let key = (symbol_short!("fund"), project_id, milestone_id);
    env.storage()
        .persistent()
        .set(&key, &funded);
}

fn storage_is_milestone_approved(env: &Env, project_id: u64, milestone_id: u32) -> bool {
    let key = (symbol_short!("appr"), project_id, milestone_id);
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(false)
}

fn storage_set_milestone_approved(env: &Env, project_id: u64, milestone_id: u32, approved: bool) {
    let key = (symbol_short!("appr"), project_id, milestone_id);
    env.storage()
        .persistent()
        .set(&key, &approved);
}

fn storage_is_milestone_disputed(env: &Env, project_id: u64, milestone_id: u32) -> bool {
    let key = (symbol_short!("disp"), project_id, milestone_id);
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(false)
}

fn storage_set_milestone_disputed(env: &Env, project_id: u64, milestone_id: u32, disputed: bool) {
    let key = (symbol_short!("disp"), project_id, milestone_id);
    env.storage()
        .persistent()
        .set(&key, &disputed);
}

fn storage_is_milestone_released(env: &Env, project_id: u64, milestone_id: u32) -> bool {
    let key = (symbol_short!("rele"), project_id, milestone_id);
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(false)
}

fn storage_set_milestone_released(env: &Env, project_id: u64, milestone_id: u32, released: bool) {
    let key = (symbol_short!("rele"), project_id, milestone_id);
    env.storage()
        .persistent()
        .set(&key, &released);
}

fn storage_is_milestone_resolved(env: &Env, project_id: u64, milestone_id: u32) -> bool {
    let key = (symbol_short!("res"), project_id, milestone_id);
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(false)
}

fn storage_set_milestone_resolved(env: &Env, project_id: u64, milestone_id: u32, resolved: bool) {
    let key = (symbol_short!("res"), project_id, milestone_id);
    env.storage()
        .persistent()
        .set(&key, &resolved);
}

fn storage_get_project_balance(env: &Env, project_id: u64) -> i128 {
    let key = (symbol_short!("bal"), project_id);
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(0)
}

fn storage_set_project_cancelled(env: &Env, project_id: u64, cancelled: bool) {
    let key = (symbol_short!("canc"), project_id);
    env.storage()
        .persistent()
        .set(&key, &cancelled);
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_initialization() {
        let env = soroban_sdk::Env::default();
        let admin = Address::random(&env);

        env.mock_all_auths();

        assert_eq!(
            EscrowMilestone::initialize(env, admin),
            Ok(())
        );
    }

    #[test]
    fn test_create_project() {
        let env = soroban_sdk::Env::default();
        let admin = Address::random(&env);
        let buyer = Address::random(&env);
        let provider = Address::random(&env);
        let arbitrator = Address::random(&env);
        let token = Address::random(&env);

        env.mock_all_auths();

        EscrowMilestone::initialize(env.clone(), admin).unwrap();
        let project_id = EscrowMilestone::create_project(
            env,
            buyer,
            provider,
            arbitrator,
            token
        ).unwrap();

        assert_eq!(project_id, 0);
    }

    #[test]
    fn test_milestone_approval_flow() {
        let env = soroban_sdk::Env::default();
        let admin = Address::random(&env);
        let buyer = Address::random(&env);
        let provider = Address::random(&env);
        let arbitrator = Address::random(&env);
        let token = Address::random(&env);

        env.mock_all_auths();

        EscrowMilestone::initialize(env.clone(), admin).unwrap();
        let project_id = EscrowMilestone::create_project(
            env.clone(),
            buyer.clone(),
            provider,
            arbitrator,
            token
        ).unwrap();

        let future_time = 1000000u64;
        EscrowMilestone::add_milestone(
            env.clone(),
            project_id,
            0,
            soroban_sdk::Vec::new(&env),
            100,
            future_time
        ).unwrap();
    }
}
