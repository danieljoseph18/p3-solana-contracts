use anchor_lang::prelude::*;
use instructions::*;

pub mod errors;
pub mod instructions;
pub mod state;

declare_id!("VaULT11111111111111111111111111111111111111");

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
