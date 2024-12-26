use crate::state::{PoolState, UserState};
use anchor_lang::prelude::*;

/// Update the user’s pending rewards right before their LP balance changes.
pub fn update_user_rewards(
    pool_state: &mut Account<PoolState>,
    user_state: &mut Account<UserState>,
) -> Result<()> {
    // If user has zero LP, there's no new accrual
    if user_state.lp_token_balance == 0 {
        user_state.last_claim_timestamp = Clock::get()?.unix_timestamp as u64;
        return Ok(());
    }

    let now = Clock::get()?.unix_timestamp as u64;
    let last_claim = user_state.last_claim_timestamp;

    // If before start or after end, no accrual
    if now <= pool_state.reward_start_time || last_claim >= pool_state.reward_end_time {
        user_state.last_claim_timestamp = now;
        return Ok(());
    }

    // Bound the claim window
    let claim_start = last_claim.max(pool_state.reward_start_time);
    let claim_end = now.min(pool_state.reward_end_time);
    let time_elapsed = claim_end.saturating_sub(claim_start);

    if time_elapsed == 0 {
        user_state.last_claim_timestamp = now;
        return Ok(());
    }

    // Calculate newly accrued rewards for the user
    // pending = (lp_token_balance * tokens_per_interval) * time_elapsed
    let newly_accrued = user_state
        .lp_token_balance
        .checked_mul(pool_state.tokens_per_interval)
        .ok_or_else(|| error!(crate::errors::VaultError::MathError))?
        .checked_mul(time_elapsed)
        .ok_or_else(|| error!(crate::errors::VaultError::MathError))?;

    // 1) Add the newly accrued to user_state.pending_rewards
    user_state.pending_rewards = user_state
        .pending_rewards
        .checked_add(newly_accrued)
        .ok_or_else(|| error!(crate::errors::VaultError::MathError))?;

    // 2) Update user’s last claim timestamp
    user_state.last_claim_timestamp = now;

    Ok(())
}
