use crate::{errors::VaultError, state::*};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct ClosePool<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [b"pool-state".as_ref()],
        bump,
        constraint = pool_state.admin == admin.key() @ VaultError::Unauthorized,
        close = admin
    )]
    pub pool_state: Account<'info, PoolState>,

    pub system_program: Program<'info, System>,
}

pub fn handle_close_pool(ctx: Context<ClosePool>) -> Result<()> {
    // Verify the signer is the admin
    require_keys_eq!(
        ctx.accounts.admin.key(),
        ctx.accounts.pool_state.admin,
        VaultError::Unauthorized
    );

    // The account will be automatically closed and rent returned to admin
    // because of the `close = admin` constraint
    msg!("Pool state account closed successfully");
    Ok(())
}
