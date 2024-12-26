use crate::{errors::VaultError, state::*};
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};
use chainlink_solana as chainlink;

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

    /// CHECK: This is the Chainlink program's address
    pub chainlink_program: AccountInfo<'info>,

    /// CHECK: This is the Chainlink feed account for SOL/USD
    pub chainlink_feed: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}

pub fn handle_admin_withdraw(ctx: Context<AdminWithdraw>, amount: u64) -> Result<()> {
    // 1) First, get an immutable reference to the pool_state's AccountInfo
    //    for use as the "authority" in our CPI.
    let pool_state_info = ctx.accounts.pool_state.to_account_info();

    // 2) Now borrow the pool_state data *mutably* in a separate binding.
    //    We do this after we’ve already taken the AccountInfo above.
    let pool_state = &mut ctx.accounts.pool_state;

    // Check admin authority
    require_keys_eq!(
        ctx.accounts.admin.key(),
        pool_state.admin,
        VaultError::Unauthorized
    );

    // If withdrawing SOL, fetch/update the current SOL/USD price
    if ctx.accounts.vault_account.key() == pool_state.sol_vault {
        let round = chainlink::latest_round_data(
            ctx.accounts.chainlink_program.to_account_info(),
            ctx.accounts.chainlink_feed.to_account_info(),
        )?;
        pool_state.sol_usd_price = round.answer;
    }

    // Transfer from the vault to the admin.
    // Notice we’re using `pool_state_info` (immutable AccountInfo)
    // as the authority for the vault’s PDA.
    let cpi_ctx = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        Transfer {
            from: ctx.accounts.vault_account.to_account_info(),
            to: ctx.accounts.admin_token_account.to_account_info(),
            authority: pool_state_info,
        },
    );
    token::transfer(cpi_ctx.with_signer(&[]), amount)?;

    // Decrement deposited tokens
    if ctx.accounts.vault_account.key() == pool_state.sol_vault {
        pool_state.sol_deposited = pool_state
            .sol_deposited
            .checked_sub(amount)
            .ok_or_else(|| error!(VaultError::MathError))?;
    } else if ctx.accounts.vault_account.key() == pool_state.usdc_vault {
        pool_state.usdc_deposited = pool_state
            .usdc_deposited
            .checked_sub(amount)
            .ok_or_else(|| error!(VaultError::MathError))?;
    } else {
        return err!(VaultError::InvalidTokenMint);
    }

    msg!("Admin withdrew {} tokens from vault.", amount);
    Ok(())
}
