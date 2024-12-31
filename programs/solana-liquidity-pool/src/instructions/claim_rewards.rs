use crate::instructions::helpers::update_user_rewards;
use crate::state::*;
use crate::{errors::VaultError, RewardsClaimed};
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

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
        constraint = usdc_reward_vault.key() == pool_state.usdc_reward_vault
    )]
    pub usdc_reward_vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = user_usdc_account.owner == user.key(),
        constraint = user_usdc_account.mint == usdc_reward_vault.mint
    )]
    pub user_usdc_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn handle_claim_rewards(ctx: Context<ClaimRewards>) -> Result<()> {
    // First, grab an immutable reference to the pool_state AccountInfo
    // to use as the authority in the token transfer.
    let pool_state_info = ctx.accounts.pool_state.to_account_info();

    // Next, create a mutable reference to the PoolState account data.
    let pool_state = &mut ctx.accounts.pool_state;
    let user_state = &mut ctx.accounts.user_state;

    // 1) Update user’s accrual to get an up-to-date `pending_rewards`
    update_user_rewards(pool_state, user_state)?;

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
            authority: pool_state_info, // the account that signs for the vault
        },
    );
    token::transfer(
        cpi_ctx.with_signer(&[&[b"pool-state".as_ref(), &[ctx.bumps.pool_state]]]),
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
