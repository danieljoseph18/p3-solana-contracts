use crate::instructions::helpers::update_user_rewards;
use crate::state::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};

#[derive(Accounts)]
pub struct ClaimRewards<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(mut)]
    pub pool_state: Account<'info, PoolState>,

    #[account(
        mut,
        seeds = [b"user-state".as_ref(), user.key().as_ref()],
        bump
    )]
    pub user_state: Account<'info, UserState>,

    #[account(mut)]
    pub usdc_reward_vault: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user_usdc_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn handle_claim_rewards(ctx: Context<ClaimRewards>) -> Result<()> {
    // We just call the same logic used in deposit/withdraw "update_user_rewards"
    let pool_state = &mut ctx.accounts.pool_state;
    let user_state = &mut ctx.accounts.user_state;

    // The update_user_rewards function does the actual "claim logic".
    // However, that’s set up to do a direct “transfer” inside (which we stubbed out).
    // So you can implement it inline here or unify logic in a real scenario.

    update_user_rewards(pool_state, user_state)?;

    // If you wanted to do a direct CPI transfer here for actual “pending” tokens:
    // ...
    // For demonstration, we rely on the logic in `update_user_rewards`.

    msg!("Claimed rewards for user: {}", ctx.accounts.user.key());
    Ok(())
}
