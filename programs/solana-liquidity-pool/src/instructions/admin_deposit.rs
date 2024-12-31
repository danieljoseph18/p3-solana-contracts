use crate::{
    errors::VaultError, state::*, CHAINLINK_PROGRAM_ID, DEVNET_SOL_PRICE_FEED,
    MAINNET_SOL_PRICE_FEED,
};
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};
use chainlink_solana as chainlink;

#[derive(Accounts)]
pub struct AdminDeposit<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [b"pool-state".as_ref()],
        bump,
        constraint = pool_state.admin == admin.key() @ VaultError::Unauthorized
    )]
    pub pool_state: Account<'info, PoolState>,

    #[account(mut)]
    pub admin_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub vault_account: Account<'info, TokenAccount>,

    /// CHECK: Validated in constraint
    #[account(address = CHAINLINK_PROGRAM_ID.parse::<Pubkey>().unwrap())]
    pub chainlink_program: AccountInfo<'info>,

    /// CHECK: Validated in constraint
    #[account(
        address = if cfg!(feature = "devnet") {
            DEVNET_SOL_PRICE_FEED
        } else {
            MAINNET_SOL_PRICE_FEED
        }.parse::<Pubkey>().unwrap()
    )]
    pub chainlink_feed: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}

pub fn handle_admin_deposit(ctx: Context<AdminDeposit>, amount: u64) -> Result<()> {
    let pool_state = &mut ctx.accounts.pool_state;

    // Ensure the signer is the admin
    require_keys_eq!(
        ctx.accounts.admin.key(),
        pool_state.admin,
        VaultError::Unauthorized
    );

    // If depositing SOL, fetch/update the current SOL/USD price
    if ctx.accounts.vault_account.key() == pool_state.sol_vault {
        let round = chainlink::latest_round_data(
            ctx.accounts.chainlink_program.to_account_info(),
            ctx.accounts.chainlink_feed.to_account_info(),
        )?;
        pool_state.sol_usd_price = round.answer;
    }

    // Transfer tokens from the admin to the vault
    let transfer_cpi_ctx = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        Transfer {
            from: ctx.accounts.admin_token_account.to_account_info(),
            to: ctx.accounts.vault_account.to_account_info(),
            authority: ctx.accounts.admin.to_account_info(),
        },
    );
    token::transfer(transfer_cpi_ctx, amount)?;

    // Update the pool's record of how many SOL or USDC tokens are deposited
    if ctx.accounts.vault_account.key() == pool_state.sol_vault {
        pool_state.sol_deposited = pool_state
            .sol_deposited
            .checked_add(amount)
            .ok_or_else(|| error!(VaultError::MathError))?;
    } else if ctx.accounts.vault_account.key() == pool_state.usdc_vault {
        pool_state.usdc_deposited = pool_state
            .usdc_deposited
            .checked_add(amount)
            .ok_or_else(|| error!(VaultError::MathError))?;
    } else {
        return err!(VaultError::InvalidTokenMint);
    }

    msg!("Admin deposited {} tokens into vault.", amount);
    Ok(())
}
