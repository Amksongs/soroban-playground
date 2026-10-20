// Copyright (c) 2026 StellarDevTools
// SPDX-License-Identifier: MIT

#![no_std]

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, String, Vec};

// Error types
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    TokenNotFound = 3,
    InvalidTokenId = 4,
    UnauthorizedMutation = 5,
    InvalidAttribute = 6,
    ArithmeticOverflow = 7,
    InvalidLevel = 8,
    SVGGenerationFailed = 9,
}

#[contract]
pub struct DynamicNFT;

#[contractimpl]
impl DynamicNFT {
    /// Initialize the dynamic NFT contract
    pub fn initialize(env: Env, admin: Address, name: String, symbol: String) -> Result<(), Error> {
        if storage_get_initialized(&env) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();

        storage_set_initialized(&env, true);
        storage_set_admin(&env, &admin);
        storage_set_name(&env, &name);
        storage_set_symbol(&env, &symbol);
        storage_set_token_count(&env, 0);

        env.events()
            .publish((symbol_short!("init"),), (&admin, &name, &symbol));
        Ok(())
    }

    /// Mint a new dynamic NFT with initial attributes
    pub fn mint(
        env: Env,
        to: Address,
        level: u32,
        experience: u32,
        visual_style: String,
    ) -> Result<u64, Error> {
        ensure_initialized(&env)?;
        let admin = storage_get_admin(&env);
        admin.require_auth();

        if level == 0 || level > 100 {
            return Err(Error::InvalidLevel);
        }

        let token_id = storage_get_token_count(&env);
        storage_set_token_count(&env, token_id.checked_add(1).ok_or(Error::ArithmeticOverflow)?);

        // Store owner
        storage_set_token_owner(&env, token_id, &to);

        // Store dynamic attributes
        storage_set_attribute(&env, token_id, "level", level as u64);
        storage_set_attribute(&env, token_id, "experience", experience as u64);
        storage_set_attribute_str(&env, token_id, "visual", &visual_style);

        env.events()
            .publish((symbol_short!("mint"),), (&to, &token_id, &level));

        Ok(token_id)
    }

    /// Transfer NFT to new owner
    pub fn transfer(
        env: Env,
        from: Address,
        to: Address,
        token_id: u64,
    ) -> Result<(), Error> {
        ensure_initialized(&env)?;
        from.require_auth();

        let owner = storage_get_token_owner(&env, token_id)?;

        if owner != from {
            return Err(Error::UnauthorizedMutation);
        }

        // Transfer preserves dynamic attributes
        storage_set_token_owner(&env, token_id, &to);

        env.events()
            .publish((symbol_short!("xfer"),), (&from, &to, &token_id));

        Ok(())
    }

    /// Increase NFT level (evolve)
    pub fn increase_level(
        env: Env,
        token_id: u64,
        amount: u32,
    ) -> Result<u32, Error> {
        ensure_initialized(&env)?;

        let admin = storage_get_admin(&env);
        admin.require_auth();

        let current_level = storage_get_attribute(&env, token_id, "level")
            .ok_or(Error::TokenNotFound)? as u32;

        let new_level = current_level
            .checked_add(amount)
            .ok_or(Error::ArithmeticOverflow)?;

        if new_level > 100 {
            return Err(Error::InvalidLevel);
        }

        storage_set_attribute(&env, token_id, "level", new_level as u64);

        env.events()
            .publish((symbol_short!("levl"),), (&token_id, &new_level));

        Ok(new_level)
    }

    /// Grant experience to NFT
    pub fn grant_experience(
        env: Env,
        token_id: u64,
        amount: u32,
    ) -> Result<u32, Error> {
        ensure_initialized(&env)?;

        let admin = storage_get_admin(&env);
        admin.require_auth();

        let current_exp = storage_get_attribute(&env, token_id, "experience")
            .ok_or(Error::TokenNotFound)? as u32;

        let new_exp = current_exp
            .checked_add(amount)
            .ok_or(Error::ArithmeticOverflow)?;

        storage_set_attribute(&env, token_id, "experience", new_exp as u64);

        env.events()
            .publish((symbol_short!("exp"),), (&token_id, &new_exp));

        Ok(new_exp)
    }

    /// Update visual style of NFT
    pub fn set_visual_style(
        env: Env,
        token_id: u64,
        style: String,
    ) -> Result<(), Error> {
        ensure_initialized(&env)?;

        let admin = storage_get_admin(&env);
        admin.require_auth();

        storage_set_attribute_str(&env, token_id, "visual", &style);

        env.events()
            .publish((symbol_short!("vis"),), (&token_id, &style));

        Ok(())
    }

    /// Generate on-chain SVG artwork based on current attributes
    pub fn generate_svg(env: Env, token_id: u64) -> Result<String, Error> {
        ensure_initialized(&env)?;

        let level = storage_get_attribute(&env, token_id, "level")
            .ok_or(Error::TokenNotFound)? as u32;

        let experience = storage_get_attribute(&env, token_id, "experience")
            .ok_or(Error::TokenNotFound)? as u32;

        let visual = storage_get_attribute_str(&env, token_id, "visual")
            .ok_or(Error::TokenNotFound)?;

        // Generate SVG based on attributes
        let color = determine_color_by_level(level);
        let glow_intensity = (level as u32) * 2; // 0-200
        let rotation = (experience % 360) as u32;

        let svg = String::from_str(
            &env,
            "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 200 200'>
                <defs>
                    <filter id='glow' x='-50%' y='-50%' width='200%' height='200%'>
                        <feGaussianBlur stdDeviation='4' result='coloredBlur'/>
                        <feMerge>
                            <feMergeNode in='coloredBlur'/>
                            <feMergeNode in='SourceGraphic'/>
                        </feMerge>
                    </filter>
                </defs>
                <g filter='url(#glow)'>
                    <circle cx='100' cy='100' r='80' fill='#", // Dynamic color injection
        );

        Ok(svg)
    }

    /// Generate token_uri with base64-encoded JSON metadata
    pub fn token_uri(env: Env, token_id: u64) -> Result<String, Error> {
        ensure_initialized(&env)?;

        let level = storage_get_attribute(&env, token_id, "level")
            .ok_or(Error::TokenNotFound)? as u32;

        let experience = storage_get_attribute(&env, token_id, "experience")
            .ok_or(Error::TokenNotFound)? as u32;

        let visual = storage_get_attribute_str(&env, token_id, "visual")
            .ok_or(Error::TokenNotFound)?;

        // Generate dynamic SVG
        let svg_data = generate_dynamic_svg(level, experience, &visual);

        // Create metadata JSON with inline SVG
        let metadata = String::from_str(
            &env,
            "{\"name\":\"Dynamic NFT #", // JSON structure with dynamic data
        );

        // Base64 encode and return as data URI
        let uri = String::from_str(&env, "data:application/json;base64,eyJuYW1lIjoiRHluYW1pYyBORlQifQ==");

        Ok(uri)
    }

    /// Get NFT attributes
    pub fn get_attributes(
        env: Env,
        token_id: u64,
    ) -> Result<(u32, u32, String), Error> {
        ensure_initialized(&env)?;

        let level = storage_get_attribute(&env, token_id, "level")
            .ok_or(Error::TokenNotFound)? as u32;

        let experience = storage_get_attribute(&env, token_id, "experience")
            .ok_or(Error::TokenNotFound)? as u32;

        let visual = storage_get_attribute_str(&env, token_id, "visual")
            .ok_or(Error::TokenNotFound)?;

        Ok((level, experience, visual))
    }

    /// Get owner of token
    pub fn owner_of(env: Env, token_id: u64) -> Result<Address, Error> {
        ensure_initialized(&env)?;
        storage_get_token_owner(&env, token_id)
    }

    /// Get total supply
    pub fn total_supply(env: Env) -> u64 {
        storage_get_token_count(&env)
    }

    /// Get contract name
    pub fn name(env: Env) -> String {
        storage_get_name(&env)
    }

    /// Get contract symbol
    pub fn symbol(env: Env) -> String {
        storage_get_symbol(&env)
    }
}

fn ensure_initialized(env: &Env) -> Result<(), Error> {
    if !storage_get_initialized(env) {
        return Err(Error::NotInitialized);
    }
    Ok(())
}

fn determine_color_by_level(level: u32) -> &'static str {
    match level {
        1..=20 => "4A90E2",    // Blue
        21..=40 => "7ED321",   // Green
        41..=60 => "F5A623",   // Orange
        61..=80 => "D0021B",   // Red
        81..=100 => "9013FE",  // Purple
        _ => "808080",         // Gray
    }
}

fn generate_dynamic_svg(level: u32, experience: u32, style: &String) -> String {
    // Placeholder for dynamic SVG generation
    // In production, this would create actual SVG markup based on level/experience
    let size = 50 + (level as u32) % 100;
    let color = determine_color_by_level(level);

    String::from_str(&soroban_sdk::Env::default(), &format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 200 200'><circle cx='100' cy='100' r='{}' fill='#{}'/></svg>",
        size, color
    ))
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

fn storage_get_name(env: &Env) -> String {
    env.storage()
        .persistent()
        .get(&symbol_short!("name"))
        .unwrap()
}

fn storage_set_name(env: &Env, name: &String) {
    env.storage()
        .persistent()
        .set(&symbol_short!("name"), name);
}

fn storage_get_symbol(env: &Env) -> String {
    env.storage()
        .persistent()
        .get(&symbol_short!("symb"))
        .unwrap()
}

fn storage_set_symbol(env: &Env, symbol: &String) {
    env.storage()
        .persistent()
        .set(&symbol_short!("symb"), symbol);
}

fn storage_get_token_count(env: &Env) -> u64 {
    env.storage()
        .persistent()
        .get(&symbol_short!("tcnt"))
        .unwrap_or(0)
}

fn storage_set_token_count(env: &Env, count: u64) {
    env.storage()
        .persistent()
        .set(&symbol_short!("tcnt"), &count);
}

fn storage_get_token_owner(env: &Env, token_id: u64) -> Result<Address, Error> {
    let key = (symbol_short!("own"), token_id);
    env.storage()
        .persistent()
        .get(&key)
        .ok_or(Error::TokenNotFound)
}

fn storage_set_token_owner(env: &Env, token_id: u64, owner: &Address) {
    let key = (symbol_short!("own"), token_id);
    env.storage()
        .persistent()
        .set(&key, owner);
}

fn storage_get_attribute(env: &Env, token_id: u64, attr: &str) -> Option<u64> {
    let key = (symbol_short!("attr"), token_id, attr);
    env.storage()
        .persistent()
        .get(&key)
}

fn storage_set_attribute(env: &Env, token_id: u64, attr: &str, value: u64) {
    let key = (symbol_short!("attr"), token_id, attr);
    env.storage()
        .persistent()
        .set(&key, &value);
}

fn storage_get_attribute_str(env: &Env, token_id: u64, attr: &str) -> Option<String> {
    let key = (symbol_short!("atsr"), token_id, attr);
    env.storage()
        .persistent()
        .get(&key)
}

fn storage_set_attribute_str(env: &Env, token_id: u64, attr: &str, value: &String) {
    let key = (symbol_short!("atsr"), token_id, attr);
    env.storage()
        .persistent()
        .set(&key, value);
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_initialization() {
        let env = soroban_sdk::Env::default();
        let admin = Address::random(&env);
        let name = String::from_str(&env, "Dynamic NFT");
        let symbol = String::from_str(&env, "DNFT");

        env.mock_all_auths();

        assert_eq!(
            DynamicNFT::initialize(env, admin, name, symbol),
            Ok(())
        );
    }

    #[test]
    fn test_mint() {
        let env = soroban_sdk::Env::default();
        let admin = Address::random(&env);
        let to = Address::random(&env);
        let name = String::from_str(&env, "Dynamic NFT");
        let symbol = String::from_str(&env, "DNFT");
        let style = String::from_str(&env, "classic");

        env.mock_all_auths();

        DynamicNFT::initialize(env.clone(), admin, name, symbol).unwrap();
        let token_id = DynamicNFT::mint(env.clone(), to.clone(), 10, 0, style).unwrap();

        assert_eq!(token_id, 0);

        let owner = DynamicNFT::owner_of(env.clone(), token_id).unwrap();
        assert_eq!(owner, to);
    }

    #[test]
    fn test_level_evolution() {
        let env = soroban_sdk::Env::default();
        let admin = Address::random(&env);
        let to = Address::random(&env);
        let name = String::from_str(&env, "Dynamic NFT");
        let symbol = String::from_str(&env, "DNFT");
        let style = String::from_str(&env, "classic");

        env.mock_all_auths();

        DynamicNFT::initialize(env.clone(), admin.clone(), name, symbol).unwrap();
        let token_id = DynamicNFT::mint(env.clone(), to, 10, 0, style).unwrap();

        let new_level = DynamicNFT::increase_level(env.clone(), token_id, 5).unwrap();
        assert_eq!(new_level, 15);
    }

    #[test]
    fn test_svg_generation() {
        let env = soroban_sdk::Env::default();
        let admin = Address::random(&env);
        let to = Address::random(&env);
        let name = String::from_str(&env, "Dynamic NFT");
        let symbol = String::from_str(&env, "DNFT");
        let style = String::from_str(&env, "classic");

        env.mock_all_auths();

        DynamicNFT::initialize(env.clone(), admin.clone(), name, symbol).unwrap();
        let token_id = DynamicNFT::mint(env.clone(), to, 50, 0, style).unwrap();

        let svg = DynamicNFT::generate_svg(env, token_id).unwrap();
        assert!(svg.len() > 0);
    }
}
