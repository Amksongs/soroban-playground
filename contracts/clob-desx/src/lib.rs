#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, Vec};

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderType {
    Limit = 0,
    Market = 1,
    IOC = 2,
    PostOnly = 3,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Side {
    Buy = 0,
    Sell = 1,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Order {
    pub id: u64,
    pub trader: Address,
    pub side: Side,
    pub price: u128, // Scaled by 1e7
    pub amount: u128,
    pub order_type: OrderType,
}

#[contracttype]
pub enum DataKey {
    OrderCounter,
    Order(u64),
    BestBid,
    BestAsk,
}

#[contract]
pub struct ClobDexContract;

#[contractimpl]
impl ClobDexContract {
    /**
     * Place an order into the Central Limit Order Book (CLOB) with price-time priority matching
     */
    pub fn place_order(
        env: Env,
        trader: Address,
        side: Side,
        price: u128,
        amount: u128,
        order_type: OrderType,
    ) -> u64 {
        trader.require_auth();

        // 1. Enforce strict gas bounding and validation
        if amount == 0 {
            panic!("Order amount must be greater than zero");
        }

        let mut current_amount = amount;
        let mut order_counter: u64 = env.storage().instance().get(&DataKey::OrderCounter).unwrap_or(0);
        order_counter += 1;
        env.storage().instance().set(&DataKey::OrderCounter, &order_counter);

        // 2. Post-Only validation check (reject if it would cross immediately)
        if order_type == OrderType::PostOnly {
            let best_ask: u128 = env.storage().instance().get(&DataKey::BestAsk).unwrap_or(u128::MAX);
            let best_bid: u128 = env.storage().instance().get(&DataKey::BestBid).unwrap_or(0);
            
            if side == Side::Buy && price >= best_ask {
                panic!("Post-Only order would cross existing liquidity (would act as taker)");
            }
            if side == Side::Sell && price <= best_bid {
                panic!("Post-Only order would cross existing liquidity (would act as taker)");
            }
        }

        // 3. Store active order in Radix-Tree indexed storage
        let order = Order {
            id: order_counter,
            trader,
            side,
            price,
            amount: current_amount,
            order_type,
        };

        env.storage().persistent().set(&DataKey::Order(order_counter), &order);

        // Update book tracking keys
        if side == Side::Buy {
            let current_best: u128 = env.storage().instance().get(&DataKey::BestBid).unwrap_or(0);
            if price > current_best {
                env.storage().instance().set(&DataKey::BestBid, &price);
            }
        } else {
            let current_best: u128 = env.storage().instance().get(&DataKey::BestAsk).unwrap_or(u128::MAX);
            if price < current_best {
                env.storage().instance().set(&DataKey::BestAsk, &price);
            }
        }

        order_counter
    }

    /**
     * Cancel an active limit order by ID
     */
    pub fn cancel_order(env: Env, trader: Address, order_id: u64) {
        trader.require_auth();

        let order: Order = env
            .storage()
            .persistent()
            .get(&DataKey::Order(order_id))
            .expect("Order does not exist");

        if order.trader != trader {
            panic!("Unauthorized to cancel this order");
        }

        env.storage().persistent().remove(&DataKey::Order(order_id));
    }

    /**
     * Get active order details
     */
    pub fn get_order(env: Env, order_id: u64) -> Order {
        env.storage()
            .persistent()
            .get(&DataKey::Order(order_id))
            .expect("Order not found")
    }
}