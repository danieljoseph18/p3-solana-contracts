use crate::state::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

/// Context for initialize
#[derive(Accounts)]
#[instruction(_bump: u8)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// The PoolState (PDA) to store global info about the pool
    #[account(
        init,
        payer = admin,
        space = 8 + std::mem::size_of::<PoolState>(),
        seeds = [b"pool-state".as_ref()],
        bump
    )]
    pub pool_state: Account<'info, PoolState>,

    /// SOL vault account (if using wrapped SOL, this would be a token account)
    /// Here, assume you've already created the vault outside or are about to
    #[account(mut)]
    pub sol_vault: Account<'info, TokenAccount>,

    /// USDC vault account
    #[account(mut)]
    pub usdc_vault: Account<'info, TokenAccount>,

    /// Reward vault for USDC
    #[account(mut)]
    pub usdc_reward_vault: Account<'info, TokenAccount>,

    /// LP token mint
    #[account(init_if_needed, payer = admin, mint::decimals = 6, mint::authority = admin)]
    pub lp_token_mint: Account<'info, Mint>,

    #[account(address = anchor_spl::token::ID)]
    pub token_program: Program<'info, Token>,

    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handle(ctx: Context<Initialize>) -> Result<()> {
    let pool_state = &mut ctx.accounts.pool_state;
    pool_state.admin = ctx.accounts.admin.key();
    pool_state.sol_vault = ctx.accounts.sol_vault.key();
    pool_state.usdc_vault = ctx.accounts.usdc_vault.key();
    pool_state.lp_token_mint = ctx.accounts.lp_token_mint.key();
    pool_state.aum_usd = 0;
    pool_state.tokens_per_interval = 0;
    pool_state.reward_start_time = 0;
    pool_state.reward_end_time = 0;
    pool_state.usdc_reward_vault = ctx.accounts.usdc_reward_vault.key();

    msg!("Pool initialized successfully.");
    Ok(())
}
