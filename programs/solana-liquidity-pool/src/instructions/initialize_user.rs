use crate::state::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct InitializeUser<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        init,
        payer = user,
        space = 8 + UserState::LEN,
        seeds = [b"user-state".as_ref(), user.key().as_ref()],
        bump
    )]
    pub user_state: Account<'info, UserState>,

    pub system_program: Program<'info, System>,
}

pub fn handle_initialize_user(ctx: Context<InitializeUser>) -> Result<()> {
    let user_state = &mut ctx.accounts.user_state;
    user_state.owner = ctx.accounts.user.key();
    user_state.lp_token_balance = 0;
    user_state.last_claim_timestamp = Clock::get()?.unix_timestamp as u64;
    user_state.pending_rewards = 0;
    user_state.previous_cumulated_reward_per_token = 0;

    msg!(
        "User state initialized successfully for: {}",
        user_state.owner
    );
    Ok(())
}
