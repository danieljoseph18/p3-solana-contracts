use anchor_lang::prelude::*;
use chainlink_solana as chainlink;

// Bring in your other modules
use instructions::*;

pub mod errors;
pub mod instructions;
pub mod state;

// Single program ID for this entire program
declare_id!("VaULT11111111111111111111111111111111111111");

/// The main vault program.
/// It includes instructions for initialize, deposit, withdraw, admin deposit/withdraw, etc.
#[program]
pub mod vault {
    use super::*;

    /// Initialize the liquidity pool
    pub fn initialize(ctx: Context<Initialize>, _bump: u8) -> Result<()> {
        instructions::initialize::handle(ctx)
    }

    /// Deposit SOL or USDC into the pool
    pub fn deposit(ctx: Context<Deposit>, token_amount: u64) -> Result<()> {
        instructions::deposit::handle(ctx, token_amount)
    }

    /// Withdraw tokens from the pool
    pub fn withdraw(ctx: Context<Withdraw>, lp_token_amount: u64) -> Result<()> {
        instructions::withdraw::handle(ctx, lp_token_amount)
    }

    /// Admin function to withdraw tokens (market making losses)
    pub fn admin_withdraw(ctx: Context<AdminWithdraw>, amount: u64) -> Result<()> {
        instructions::admin_withdraw::handle(ctx, amount)
    }

    /// Admin function to deposit tokens (market making profits)
    pub fn admin_deposit(ctx: Context<AdminDeposit>, amount: u64) -> Result<()> {
        instructions::admin_deposit::handle(ctx, amount)
    }

    /// Admin function to start new reward distribution
    pub fn start_rewards(
        ctx: Context<StartRewards>,
        usdc_amount: u64,
        tokens_per_interval: u64,
    ) -> Result<()> {
        instructions::start_rewards::handle(ctx, usdc_amount, tokens_per_interval)
    }

    /// Claim user rewards
    pub fn claim_rewards(ctx: Context<ClaimRewards>) -> Result<()> {
        instructions::claim_rewards::handle(ctx)
    }
}

/// A separate module for Chainlink-related instructions (not another `#[program]`).
/// We import the context structs (e.g., `Initialize`, `UpdateSolUsdPrice`) from `state.rs`.
pub mod sol_usd_price_feed {
    use super::*;
    use crate::state::{Initialize, UpdateSolUsdPrice};

    /// Initialize the PoolState with basic info. This is a separate function from
    /// the main vault's "initialize" if you only want to run Chainlink-specific code here.
    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let pool_state = &mut ctx.accounts.pool_state;
        pool_state.admin = *ctx.accounts.admin.key;
        pool_state.sol_vault = *ctx.accounts.sol_vault.key;
        pool_state.usdc_vault = *ctx.accounts.usdc_vault.key;
        pool_state.lp_token_mint = *ctx.accounts.lp_token_mint.key;
        pool_state.usdc_reward_vault = *ctx.accounts.usdc_reward_vault.key;
        Ok(())
    }

    /// Pulls the latest SOL/USD price from a Chainlink feed and updates `PoolState`.
    pub fn update_sol_usd_price(ctx: Context<UpdateSolUsdPrice>) -> Result<()> {
        let round = chainlink::latest_round_data(
            ctx.accounts.chainlink_program.to_account_info(),
            ctx.accounts.chainlink_feed.to_account_info(),
        )?;
        let price = round.answer;
        ctx.accounts.pool_state.sol_usd_price = price;
        Ok(())
    }
}
