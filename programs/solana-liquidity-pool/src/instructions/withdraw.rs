use crate::{errors::VaultError, instructions::helpers::*, state::*};
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, Mint, Token, TokenAccount, Transfer};
use chainlink_solana as chainlink;

/// Context for withdraw
#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"pool-state".as_ref()],
        bump
    )]
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

    /// CHECK: This is the Chainlink program's address
    pub chainlink_program: AccountInfo<'info>,

    /// CHECK: This is the Chainlink feed account for SOL/USD
    pub chainlink_feed: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}

pub fn handle_withdraw(ctx: Context<Withdraw>, lp_token_amount: u64) -> Result<()> {
    let pool_state = &mut ctx.accounts.pool_state;
    let user_state = &mut ctx.accounts.user_state;

    // Check the user's LP balance
    if user_state.lp_token_balance < lp_token_amount {
        return err!(VaultError::InsufficientLpBalance);
    }

    // Update any user-level rewards prior to burning LP
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

    // ----------------------------------------------------------------
    // 1) Compute the pool's total AUM in USD at this moment.
    // ----------------------------------------------------------------
    // First, if withdrawing SOL, fetch the latest SOL/USD price
    if ctx.accounts.vault_account.key() == pool_state.sol_vault {
        let round = chainlink::latest_round_data(
            ctx.accounts.chainlink_program.to_account_info(),
            ctx.accounts.chainlink_feed.to_account_info(),
        )?;
        pool_state.sol_usd_price = round.answer;
    }

    // Convert total SOL to USD (using updated pool_state.sol_usd_price)
    let total_sol_usd = get_sol_usd_value(pool_state.sol_deposited, pool_state.sol_usd_price)?;
    let current_aum = total_sol_usd
        .checked_add(pool_state.usdc_deposited)
        .ok_or_else(|| error!(VaultError::MathError))?;

    // ----------------------------------------------------------------
    // 2) Determine how much USD the burned LP tokens represent:
    //    (lp_token_amount / total LP supply) * current AUM
    // ----------------------------------------------------------------
    let lp_supply = ctx.accounts.lp_token_mint.supply.max(1);
    let withdrawal_usd_value = lp_token_amount
        .checked_mul(current_aum)
        .ok_or_else(|| error!(VaultError::MathError))?
        .checked_div(lp_supply)
        .ok_or_else(|| error!(VaultError::MathError))?;

    // ----------------------------------------------------------------
    // 3) Convert that USD value into the correct token amount (SOL or USDC).
    // ----------------------------------------------------------------
    let token_amount = if ctx.accounts.vault_account.key() == pool_state.sol_vault {
        // Withdrawing SOL
        get_sol_amount_from_usd(withdrawal_usd_value, pool_state.sol_usd_price)?
    } else if ctx.accounts.vault_account.key() == pool_state.usdc_vault {
        // USDC is 1:1 with USD
        withdrawal_usd_value
    } else {
        return err!(VaultError::InvalidTokenMint);
    };

    // ----------------------------------------------------------------
    // 4) Transfer from the vault to the user
    // ----------------------------------------------------------------
    let cpi_ctx_transfer = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        Transfer {
            from: ctx.accounts.vault_account.to_account_info(),
            to: ctx.accounts.user_token_account.to_account_info(),
            authority: pool_state.to_account_info(),
        },
    );
    token::transfer(
        cpi_ctx_transfer.with_signer(&[&[b"pool-state".as_ref(), &[ctx.bumps.pool_state]]]),
        token_amount,
    )?;

    // ----------------------------------------------------------------
    // 5) Decrement the pool's deposited token count accordingly
    // ----------------------------------------------------------------
    if ctx.accounts.vault_account.key() == pool_state.sol_vault {
        pool_state.sol_deposited = pool_state
            .sol_deposited
            .checked_sub(token_amount)
            .ok_or_else(|| error!(VaultError::MathError))?;
    } else if ctx.accounts.vault_account.key() == pool_state.usdc_vault {
        pool_state.usdc_deposited = pool_state
            .usdc_deposited
            .checked_sub(token_amount)
            .ok_or_else(|| error!(VaultError::MathError))?;
    }

    msg!(
        "Withdrawal successful. Burned {} LP tokens, returned {} units of vault token.",
        lp_token_amount,
        token_amount
    );

    Ok(())
}
