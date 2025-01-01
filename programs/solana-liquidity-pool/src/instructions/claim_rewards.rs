use crate::instructions::helpers::update_rewards;
use crate::state::*;
use crate::{errors::VaultError, RewardsClaimed};
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

#[derive(Accounts)]
pub struct ClaimRewards<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"pool-state".as_ref()],
        bump
    )]
    pub pool_state: Account<'info, PoolState>,

    #[account(
        mut,
        seeds = [b"user-state".as_ref(), user.key().as_ref()],
        bump,
        constraint = user_state.owner == user.key()
    )]
    pub user_state: Account<'info, UserState>,

    #[account(
        mut,
        constraint = usdc_reward_vault.key() == pool_state.usdc_reward_vault,
        constraint = usdc_reward_vault.owner == pool_state.key() @ VaultError::InvalidOwner
    )]
    pub usdc_reward_vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = user_usdc_account.owner == user.key(),
        constraint = user_usdc_account.mint == usdc_reward_vault.mint
    )]
    pub user_usdc_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,

    #[account(
        constraint = lp_token_mint.key() == pool_state.lp_token_mint @ VaultError::InvalidTokenMint
    )]
    pub lp_token_mint: Account<'info, Mint>,
}

pub fn handle_claim_rewards(ctx: Context<ClaimRewards>) -> Result<()> {
    // Store validation values up front
    let now = Clock::get()?.unix_timestamp as u64;
    let reward_start_time = ctx.accounts.pool_state.reward_start_time;
    let total_rewards_deposited = ctx.accounts.pool_state.total_rewards_deposited;
    let total_rewards_claimed = ctx.accounts.pool_state.total_rewards_claimed;
    let vault_amount = ctx.accounts.usdc_reward_vault.amount;
    let pool_state_bump = ctx.bumps.pool_state;

    // Do validation checks with stored values
    require!(now >= reward_start_time, VaultError::RewardsNotStarted);

    require!(
        ctx.accounts.user_state.lp_token_balance > 0,
        VaultError::NoLPTokens
    );

    let available = total_rewards_deposited.saturating_sub(total_rewards_claimed);

    require!(
        vault_amount >= available,
        VaultError::InsufficientRewardBalance
    );

    // Now take mutable references for state updates
    let pool_state = &mut ctx.accounts.pool_state;
    let user_state = &mut ctx.accounts.user_state;

    // 1) Update user’s accrual to get an up-to-date `pending_rewards`
    update_rewards(pool_state, user_state, &ctx.accounts.lp_token_mint)?;

    // 2) The user now has some "pending" amount stored locally
    let pending = user_state.pending_rewards;
    if pending == 0 {
        msg!("No rewards to claim.");
        return Ok(());
    }

    // 3) Check how much is still available in the reward pool
    let available = pool_state
        .total_rewards_deposited
        .saturating_sub(pool_state.total_rewards_claimed);

    // Add vault balance check
    let vault_balance = ctx.accounts.usdc_reward_vault.amount;
    require!(
        vault_balance >= available,
        VaultError::InsufficientRewardBalance
    );

    // Clamp the user’s claim if not enough remains in the reward pool
    let to_claim = pending.min(available);
    if to_claim == 0 {
        msg!("No rewards left in the pool to claim.");
        return Ok(());
    }

    // 4) Transfer `to_claim` tokens from the reward vault to the user
    let cpi_ctx = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        Transfer {
            from: ctx.accounts.usdc_reward_vault.to_account_info(),
            to: ctx.accounts.user_usdc_account.to_account_info(),
            authority: pool_state.to_account_info(), // Use pool_state reference instead
        },
    );
    token::transfer(
        cpi_ctx.with_signer(&[&[b"pool-state".as_ref(), &[pool_state_bump]]]),
        to_claim,
    )?;

    // 5) Update global and user-level state
    pool_state.total_rewards_claimed = pool_state
        .total_rewards_claimed
        .checked_add(to_claim)
        .ok_or_else(|| error!(VaultError::MathError))?;

    user_state.pending_rewards = user_state
        .pending_rewards
        .checked_sub(to_claim)
        .ok_or_else(|| error!(VaultError::MathError))?;

    // Emit event for subgraph indexing
    emit!(RewardsClaimed {
        user: ctx.accounts.user.key(),
        amount: to_claim,
        timestamp: Clock::get()?.unix_timestamp,
        total_claimed: pool_state.total_rewards_claimed,
    });

    msg!(
        "User {} claimed {} USDC in rewards.",
        ctx.accounts.user.key(),
        to_claim
    );
    Ok(())
}
