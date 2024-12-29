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
    msg!(
        "Starting withdrawal of {} LP tokens (6 dec)",
        lp_token_amount
    );

    let pool_state = &mut ctx.accounts.pool_state;
    let user_state = &mut ctx.accounts.user_state;

    // Check the user's LP balance (6 decimals)
    msg!(
        "Checking user LP balance: {} (6 dec)",
        user_state.lp_token_balance
    );
    if user_state.lp_token_balance < lp_token_amount {
        msg!("Insufficient LP balance");
        return err!(VaultError::InsufficientLpBalance);
    }

    // Update any user-level rewards prior to burning LP
    msg!("Updating user rewards before burning LP tokens");
    update_user_rewards(pool_state, user_state)?;

    // Burn the LP tokens (6 decimals, matching USD representation)
    msg!("Burning {} LP tokens", lp_token_amount);
    let cpi_ctx_burn = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        Burn {
            mint: ctx.accounts.lp_token_mint.to_account_info(),
            from: ctx.accounts.user_lp_token_account.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        },
    );
    token::burn(cpi_ctx_burn, lp_token_amount)?;
    msg!("LP tokens burned successfully");

    // Adjust user's recorded LP balance (6 decimals)
    user_state.lp_token_balance = user_state
        .lp_token_balance
        .checked_sub(lp_token_amount)
        .ok_or_else(|| error!(VaultError::MathError))?;
    msg!(
        "Updated user LP balance to {} (6 dec)",
        user_state.lp_token_balance
    );

    // ----------------------------------------------------------------
    // 1) Compute the pool's total AUM in USD (6 decimals) at this moment.
    // ----------------------------------------------------------------
    msg!("Computing current AUM");
    // First, if withdrawing SOL, fetch the latest SOL/USD price
    if ctx.accounts.vault_account.key() == pool_state.sol_vault {
        msg!("Withdrawing SOL, fetching latest Chainlink price");
        let round = chainlink::latest_round_data(
            ctx.accounts.chainlink_program.to_account_info(),
            ctx.accounts.chainlink_feed.to_account_info(),
        )?;
        // Update stored SOL price (8 decimals from Chainlink)
        pool_state.sol_usd_price = round.answer;
        msg!("Updated SOL/USD price to {} (8 dec)", round.answer);
    }

    // Convert total SOL to USD (returns USD with 6 decimals)
    let total_sol_usd = get_sol_usd_value(pool_state.sol_deposited, pool_state.sol_usd_price)?;
    msg!("Total SOL value in USD: {} (6 dec)", total_sol_usd);
    let current_aum = total_sol_usd
        .checked_add(pool_state.usdc_deposited)
        .ok_or_else(|| error!(VaultError::MathError))?;
    msg!("Current total AUM: {} (6 dec)", current_aum);

    // ----------------------------------------------------------------
    // 2) Determine how much USD the burned LP tokens represent (6 decimals):
    //    (lp_token_amount / total LP supply) * current AUM
    // ----------------------------------------------------------------
    msg!("Calculating withdrawal value");
    let lp_supply = ctx.accounts.lp_token_mint.supply.max(1);
    msg!("Current LP supply: {}", lp_supply);
    let withdrawal_usd_value = lp_token_amount
        .checked_mul(current_aum)
        .ok_or_else(|| error!(VaultError::MathError))?
        .checked_div(lp_supply)
        .ok_or_else(|| error!(VaultError::MathError))?;
    msg!("Withdrawal value in USD: {} (6 dec)", withdrawal_usd_value);

    // ----------------------------------------------------------------
    // 3) Convert that USD value (6 decimals) into the correct token amount:
    //    - For SOL: Convert to 9 decimals
    //    - For USDC: Keep 6 decimals
    // ----------------------------------------------------------------
    msg!("Converting USD value to withdrawal token amount");
    let token_amount = if ctx.accounts.vault_account.key() == pool_state.sol_vault {
        msg!("Converting to SOL amount");
        // Convert USD (6 decimals) to SOL (9 decimals)
        get_sol_amount_from_usd(withdrawal_usd_value, pool_state.sol_usd_price)?
    } else if ctx.accounts.vault_account.key() == pool_state.usdc_vault {
        msg!("Using USDC amount (same as USD value)");
        // USDC uses 6 decimals, same as our USD representation
        withdrawal_usd_value
    } else {
        return err!(VaultError::InvalidTokenMint);
    };
    msg!("Will withdraw {} tokens", token_amount);

    // ----------------------------------------------------------------
    // 4) Transfer from the vault to the user (amount in token's native decimals)
    // ----------------------------------------------------------------
    msg!("Transferring tokens from vault to user");
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
    msg!("Token transfer successful");

    // ----------------------------------------------------------------
    // 5) Decrement the pool's deposited token count (in token's native decimals)
    // ----------------------------------------------------------------
    if ctx.accounts.vault_account.key() == pool_state.sol_vault {
        msg!("Updating pool's SOL balance");
        // Decrease SOL amount (9 decimals)
        pool_state.sol_deposited = pool_state
            .sol_deposited
            .checked_sub(token_amount)
            .ok_or_else(|| error!(VaultError::MathError))?;
        msg!(
            "Updated pool SOL balance to {} (9 dec)",
            pool_state.sol_deposited
        );
    } else if ctx.accounts.vault_account.key() == pool_state.usdc_vault {
        msg!("Updating pool's USDC balance");
        // Decrease USDC amount (6 decimals)
        pool_state.usdc_deposited = pool_state
            .usdc_deposited
            .checked_sub(token_amount)
            .ok_or_else(|| error!(VaultError::MathError))?;
        msg!(
            "Updated pool USDC balance to {} (6 dec)",
            pool_state.usdc_deposited
        );
    }

    msg!(
        "Withdrawal successful. Burned {} LP tokens (6 decimals), returned {} {} tokens.",
        lp_token_amount,
        token_amount,
        if ctx.accounts.vault_account.key() == pool_state.sol_vault {
            "SOL (9 decimals)"
        } else {
            "USDC (6 decimals)"
        }
    );

    Ok(())
}
