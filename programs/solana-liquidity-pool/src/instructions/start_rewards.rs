use crate::{errors::VaultError, state::*};
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

#[derive(Accounts)]
pub struct StartRewards<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(mut)]
    pub pool_state: Account<'info, PoolState>,

    /// Admin’s USDC token account
    #[account(mut)]
    pub admin_usdc_account: Account<'info, TokenAccount>,

    /// Program's USDC reward vault
    #[account(mut)]
    pub usdc_reward_vault: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn handle_start_rewards(
    ctx: Context<StartRewards>,
    usdc_amount: u64,
    tokens_per_interval: u64,
) -> Result<()> {
    let pool_state = &mut ctx.accounts.pool_state;
    require_keys_eq!(
        ctx.accounts.admin.key(),
        pool_state.admin,
        VaultError::Unauthorized
    );

    // Transfer USDC from admin to the program's reward vault
    let cpi_ctx = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        Transfer {
            from: ctx.accounts.admin_usdc_account.to_account_info(),
            to: ctx.accounts.usdc_reward_vault.to_account_info(),
            authority: ctx.accounts.admin.to_account_info(),
        },
    );
    token::transfer(cpi_ctx, usdc_amount)?;

    // Set reward parameters
    let now = Clock::get()?.unix_timestamp as u64;
    pool_state.tokens_per_interval = tokens_per_interval;
    pool_state.reward_start_time = now;
    pool_state.reward_end_time = now
        .checked_add(604800)
        .ok_or_else(|| error!(VaultError::MathError))?;

    msg!("Started new reward distribution.");
    Ok(())
}
