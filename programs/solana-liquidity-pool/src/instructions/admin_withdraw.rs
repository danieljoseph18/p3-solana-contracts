use crate::{errors::VaultError, state::*};
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

#[derive(Accounts)]
pub struct AdminWithdraw<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(mut)]
    pub pool_state: Account<'info, PoolState>,

    #[account(mut)]
    pub vault_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub admin_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn handle_admin_withdraw(ctx: Context<AdminWithdraw>, amount: u64) -> Result<()> {
    let pool_state = &mut ctx.accounts.pool_state;

    // Check admin authority
    require_keys_eq!(
        ctx.accounts.admin.key(),
        pool_state.admin,
        VaultError::Unauthorized
    );

    // Transfer from vault to admin
    let cpi_ctx = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        Transfer {
            from: ctx.accounts.vault_account.to_account_info(),
            to: ctx.accounts.admin_token_account.to_account_info(),
            authority: ctx.accounts.admin.to_account_info(),
        },
    );
    token::transfer(cpi_ctx.with_signer(&[]), amount)?;

    // Update AUM
    // Check which vault is being withdrawn from:
    if ctx.accounts.vault_account.key() == pool_state.sol_vault {
        pool_state.aum_usd = pool_state
            .aum_usd
            .checked_sub(crate::state::get_sol_usd_value(
                amount,
                pool_state.sol_usd_price,
            )?)
            .ok_or_else(|| error!(VaultError::MathError))?;
    } else if ctx.accounts.vault_account.key() == pool_state.usdc_vault {
        pool_state.aum_usd = pool_state
            .aum_usd
            .checked_sub(amount)
            .ok_or_else(|| error!(VaultError::MathError))?;
    } else {
        return err!(VaultError::InvalidTokenMint);
    }

    msg!("Admin withdrew {} tokens from vault.", amount);
    Ok(())
}
