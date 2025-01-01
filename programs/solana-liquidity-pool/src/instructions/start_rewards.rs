use crate::{errors::VaultError, state::*};
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

#[derive(Accounts)]
pub struct StartRewards<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [b"pool-state".as_ref()],
        bump,
        constraint = pool_state.admin == admin.key() @ VaultError::Unauthorized
    )]
    pub pool_state: Account<'info, PoolState>,

    /// Admin's USDC token account
    #[account(mut)]
    pub admin_usdc_account: Account<'info, TokenAccount>,

    /// Program's USDC reward vault
    #[account(mut)]
    pub usdc_reward_vault: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn handle_start_rewards(
    ctx: Context<StartRewards>,
    usdc_amount: u64,          // Total rewards for the period
    _tokens_per_interval: u64, // We'll calculate this ourselves
) -> Result<()> {
    let pool_state = &mut ctx.accounts.pool_state;
    require_keys_eq!(
        ctx.accounts.admin.key(),
        pool_state.admin,
        VaultError::Unauthorized
    );

    // Transfer USDC from admin to reward vault
    let cpi_ctx = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        Transfer {
            from: ctx.accounts.admin_usdc_account.to_account_info(),
            to: ctx.accounts.usdc_reward_vault.to_account_info(),
            authority: ctx.accounts.admin.to_account_info(),
        },
    );
    token::transfer(cpi_ctx, usdc_amount)?;

    // Calculate tokens per interval (per second)
    let tokens_per_interval = usdc_amount
        .checked_div(604800) // One week in seconds
        .ok_or(VaultError::MathError)?;

    // Update state
    pool_state.total_rewards_deposited = usdc_amount;
    pool_state.total_rewards_claimed = 0;
    pool_state.tokens_per_interval = tokens_per_interval;
    pool_state.last_distribution_time = Clock::get()?.unix_timestamp as u64;

    Ok(())
}
