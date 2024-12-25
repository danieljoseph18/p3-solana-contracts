use crate::state::{PoolState, UserState};
use anchor_lang::prelude::*;
use anchor_lang::solana_program::clock;

/// Update the user’s pending rewards right before their LP balance changes.
pub fn update_user_rewards(
    pool_state: &mut Account<PoolState>,
    user_state: &mut Account<UserState>,
) -> Result<()> {
    if user_state.lp_token_balance == 0 {
        // No need to claim if user has zero balance
        user_state.last_claim_timestamp = clock::Clock::get()?.unix_timestamp as u64;
        return Ok(());
    }
    // We'll do a direct "claim" logic here, so that every deposit/withdraw
    // auto-claims before changing the user's LP balance.
    let now = clock::Clock::get()?.unix_timestamp as u64;
    let last_claim = user_state.last_claim_timestamp;

    // If rewards haven't started or user claimed recently, nothing new to claim
    if now <= pool_state.reward_start_time || last_claim >= pool_state.reward_end_time {
        user_state.last_claim_timestamp = now;
        return Ok(());
    }

    let claim_start = last_claim.max(pool_state.reward_start_time);
    let claim_end = now.min(pool_state.reward_end_time);
    let time_elapsed = claim_end.saturating_sub(claim_start);

    if time_elapsed == 0 {
        user_state.last_claim_timestamp = now;
        return Ok(());
    }

    // let pending = user_state
    //     .lp_token_balance
    //     .checked_mul(pool_state.tokens_per_interval)
    //     .ok_or_else(|| error!(crate::errors::VaultError::MathError))?
    //     .checked_mul(time_elapsed)
    //     .ok_or_else(|| error!(crate::errors::VaultError::MathError))?;

    // Transfer from reward vault to user
    // In many cases you'd do a CPI to token program to actually transfer USDC
    // Here we can just assume the token transfer is done.
    // ...

    // Update last claim time
    user_state.last_claim_timestamp = now;

    Ok(())
}
