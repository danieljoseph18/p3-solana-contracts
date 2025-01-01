use crate::{
    errors::VaultError,
    state::{PoolState, UserState},
};
use anchor_lang::prelude::*;
use anchor_spl::token::Mint;

pub fn update_rewards(
    pool_state: &mut PoolState,
    user_state: &mut UserState,
    lp_token_mint: &Account<Mint>,
) -> Result<()> {
    const PRECISION: u128 = 1_000_000_000_000;

    let now = Clock::get()?.unix_timestamp as u64;

    if lp_token_mint.supply == 0 {
        pool_state.last_distribution_time = now;
        return Ok(());
    }

    let time_diff = now.saturating_sub(pool_state.last_distribution_time);
    if time_diff > 0 {
        let pending_rewards = (pool_state.tokens_per_interval as u128)
            .checked_mul(time_diff as u128)
            .ok_or(VaultError::MathError)?;

        let reward_per_token = pending_rewards
            .checked_mul(PRECISION)
            .ok_or(VaultError::MathError)?
            .checked_div(lp_token_mint.supply as u128)
            .ok_or(VaultError::MathError)?;

        pool_state.cumulative_reward_per_token = pool_state
            .cumulative_reward_per_token
            .checked_add(reward_per_token)
            .ok_or(VaultError::MathError)?;

        pool_state.last_distribution_time = now;
    }

    let user_reward = (user_state.lp_token_balance as u128)
        .checked_mul(
            pool_state
                .cumulative_reward_per_token
                .saturating_sub(user_state.previous_cumulated_reward_per_token),
        )
        .ok_or(VaultError::MathError)?
        .checked_div(PRECISION)
        .ok_or(VaultError::MathError)?;

    user_state.pending_rewards = user_state
        .pending_rewards
        .checked_add(user_reward as u64)
        .ok_or(VaultError::MathError)?;

    user_state.previous_cumulated_reward_per_token = pool_state.cumulative_reward_per_token;

    Ok(())
}
