use crate::{errors::VaultError, instructions::helpers::*, state::*};
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, Mint, Token, TokenAccount, Transfer};

/// Context for withdraw
#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(mut)]
    pub pool_state: Account<'info, PoolState>,

    /// The user's associated UserState
    #[account(
        mut,
        seeds = [b"user-state".as_ref(), user.key().as_ref()],
        bump
    )]
    pub user_state: Account<'info, UserState>,

    /// LP token mint
    #[account(mut)]
    pub lp_token_mint: Account<'info, Mint>,

    /// User's LP token account (where they hold the LP tokens to burn)
    #[account(mut)]
    pub user_lp_token_account: Account<'info, TokenAccount>,

    /// Vault for SOL or USDC
    #[account(mut)]
    pub vault_account: Account<'info, TokenAccount>,

    /// User's token account to receive the withdrawn tokens
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn handle_withdraw(ctx: Context<Withdraw>, lp_token_amount: u64) -> Result<()> {
    let pool_state = &mut ctx.accounts.pool_state;
    let user_state = &mut ctx.accounts.user_state;

    // Check user has enough LP balance
    if user_state.lp_token_balance < lp_token_amount {
        return err!(VaultError::InsufficientLpBalance);
    }

    // Update user rewards prior to burning LP
    update_user_rewards(pool_state, user_state)?;

    // Burn the LP tokens
    let cpi_ctx_burn = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        Burn {
            mint: ctx.accounts.lp_token_mint.to_account_info(),
            from: ctx.accounts.user_lp_token_account.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        },
    );
    token::burn(cpi_ctx_burn, lp_token_amount)?;

    // Adjust user's recorded LP balance
    user_state.lp_token_balance = user_state
        .lp_token_balance
        .checked_sub(lp_token_amount)
        .ok_or_else(|| error!(VaultError::MathError))?;

    // Calculate how much USD that LP token amount is worth:
    let lp_supply = ctx.accounts.lp_token_mint.supply.max(1);
    // aum_usd / total_supply * lp_token_amount
    let withdrawal_usd_value = pool_state
        .aum_usd
        .checked_mul(lp_token_amount)
        .ok_or_else(|| error!(VaultError::MathError))?
        .checked_div(lp_supply)
        .ok_or_else(|| error!(VaultError::MathError))?;

    // Convert that USD value into actual token amount
    let token_amount = if ctx.accounts.vault_account.mint == pool_state.sol_vault {
        // Then user is withdrawing SOL
        get_sol_amount_from_usd(withdrawal_usd_value, pool_state.sol_usd_price)?
    } else if ctx.accounts.vault_account.mint == pool_state.usdc_vault {
        // USDC is 1:1 with USD
        withdrawal_usd_value
    } else {
        return err!(VaultError::InvalidTokenMint);
    };

    // Transfer from vault to user
    let cpi_ctx_transfer = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        Transfer {
            from: ctx.accounts.vault_account.to_account_info(),
            to: ctx.accounts.user_token_account.to_account_info(),
            authority: pool_state.to_account_info(),
        },
    );
    token::transfer(cpi_ctx_transfer.with_signer(&[]), token_amount)?;

    // Update AUM
    pool_state.aum_usd = pool_state
        .aum_usd
        .checked_sub(withdrawal_usd_value)
        .ok_or_else(|| error!(VaultError::MathError))?;

    msg!(
        "Withdrawal successful. Burned {} LP tokens, returned {} units of vault token.",
        lp_token_amount,
        token_amount
    );

    Ok(())
}
