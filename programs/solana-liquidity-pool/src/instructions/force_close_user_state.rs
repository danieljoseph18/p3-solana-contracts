use anchor_lang::prelude::*;

use crate::{errors::VaultError, state::PoolState};

#[derive(Accounts)]
pub struct ForceCloseUserState<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [b"pool-state".as_ref()],
        bump,
    )]
    pub pool_state: Account<'info, PoolState>,

    /// CHECK: Intentionally not deserializing
    #[account(
        mut,
        seeds = [b"user-state".as_ref(), target_user.key().as_ref()],
        bump
    )]
    pub user_state: UncheckedAccount<'info>,

    /// CHECK: Just used for PDA derivation
    pub target_user: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handle_force_close_user_state(ctx: Context<ForceCloseUserState>) -> Result<()> {
    // Verify admin
    require_keys_eq!(
        ctx.accounts.admin.key(),
        ctx.accounts.pool_state.admin,
        VaultError::Unauthorized
    );

    // Transfer lamports back to admin
    let dest_starting_lamports = ctx.accounts.admin.lamports();
    **ctx.accounts.admin.lamports.borrow_mut() = dest_starting_lamports
        .checked_add(ctx.accounts.user_state.lamports())
        .unwrap();
    **ctx.accounts.user_state.lamports.borrow_mut() = 0;

    // Clear the account data
    ctx.accounts.user_state.assign(&ctx.program_id);
    ctx.accounts.user_state.realloc(0, false)?;

    msg!(
        "Force closed user state account for: {}",
        ctx.accounts.target_user.key()
    );
    Ok(())
}
