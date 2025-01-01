use crate::{errors::VaultError, state::*};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct CloseUserState<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"pool-state".as_ref()],
        bump,
    )]
    pub pool_state: Account<'info, PoolState>,

    #[account(
        mut,
        seeds = [b"user-state".as_ref(), user.key().as_ref()],
        bump,
        constraint = (user_state.owner == user.key() || pool_state.admin == user.key()) @ VaultError::Unauthorized,
        close = user
    )]
    pub user_state: Account<'info, UserState>,

    pub system_program: Program<'info, System>,
}

pub fn handle_close_user_state(ctx: Context<CloseUserState>) -> Result<()> {
    // Log who is closing the account
    if ctx.accounts.user.key() == ctx.accounts.pool_state.admin {
        msg!(
            "Admin closed user state account for user: {}",
            ctx.accounts.user_state.owner
        );
    } else {
        msg!("User closed their own state account");
    }

    Ok(())
}
