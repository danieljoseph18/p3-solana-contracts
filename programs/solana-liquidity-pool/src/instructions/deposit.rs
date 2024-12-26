use crate::{errors::VaultError, instructions::helpers::*, state::*};
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, MintTo, Token, TokenAccount, Transfer};
use chainlink_solana as chainlink;

/// Context for deposit
#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    /// Global PoolState
    #[account(mut)]
    pub pool_state: Account<'info, PoolState>,

    /// The user's token account from which they are depositing
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,

    /// Vault for either SOL (wrapped) or USDC
    #[account(mut)]
    pub vault_account: Account<'info, TokenAccount>,

    /// The user's associated UserState
    #[account(
        init_if_needed,
        payer = user,
        space = 8 + std::mem::size_of::<UserState>(),
        seeds = [b"user-state".as_ref(), user.key().as_ref()],
        bump
    )]
    pub user_state: Account<'info, UserState>,

    /// LP token mint
    #[account(mut)]
    pub lp_token_mint: Account<'info, Mint>,

    /// The user's LP token account (where minted LP tokens will go)
    #[account(mut)]
    pub user_lp_token_account: Account<'info, TokenAccount>,

    /// CHECK: This is the Chainlink program's address
    pub chainlink_program: AccountInfo<'info>,

    /// CHECK: This is the Chainlink feed account for SOL/USD
    pub chainlink_feed: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handle_deposit(ctx: Context<Deposit>, token_amount: u64) -> Result<()> {
    // For readability
    let pool_state = &mut ctx.accounts.pool_state;
    let user_state = &mut ctx.accounts.user_state;

    // Identify if this deposit is SOL or USDC.
    let sol_vault = pool_state.sol_vault;
    let usdc_vault = pool_state.usdc_vault;

    // If depositing SOL, fetch latest price from Chainlink.
    if ctx.accounts.vault_account.mint == sol_vault {
        let round = chainlink::latest_round_data(
            ctx.accounts.chainlink_program.to_account_info(),
            ctx.accounts.chainlink_feed.to_account_info(),
        )?;
        // Update stored SOL price
        pool_state.sol_usd_price = round.answer;
    }

    // Transfer tokens from user into the vault
    let transfer_cpi_ctx = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        Transfer {
            from: ctx.accounts.user_token_account.to_account_info(),
            to: ctx.accounts.vault_account.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        },
    );
    token::transfer(transfer_cpi_ctx, token_amount)?;

    // Determine how many tokens in USD were deposited.
    // Also update the pool's recorded total (sol_deposited / usdc_deposited).
    let deposit_usd = if ctx.accounts.vault_account.mint == sol_vault {
        // Increase total SOL
        pool_state.sol_deposited = pool_state
            .sol_deposited
            .checked_add(token_amount)
            .ok_or(VaultError::MathError)?;

        get_sol_usd_value(token_amount, pool_state.sol_usd_price)?
    } else if ctx.accounts.vault_account.mint == usdc_vault {
        // Increase total USDC
        pool_state.usdc_deposited = pool_state
            .usdc_deposited
            .checked_add(token_amount)
            .ok_or(VaultError::MathError)?;

        // 1:1 ratio for USDC -> USD
        token_amount
    } else {
        return err!(VaultError::InvalidTokenMint);
    };

    // Now compute the *current* AUM (in USD) based on updated totals.
    // 1) Convert total SOL to USD, 2) Add total USDC
    let total_sol_usd = get_sol_usd_value(pool_state.sol_deposited, pool_state.sol_usd_price)?;
    let current_aum = total_sol_usd
        .checked_add(pool_state.usdc_deposited)
        .ok_or(VaultError::MathError)?;

    // Figure out how many LP tokens to mint:
    //   if first deposit, deposit_usd tokens
    //   else deposit_usd * (LP supply / AUM).
    let lp_supply = ctx.accounts.lp_token_mint.supply;
    let lp_to_mint = if lp_supply == 0 {
        deposit_usd
    } else {
        deposit_usd
            .checked_mul(lp_supply)
            .ok_or(VaultError::MathError)?
            .checked_div(current_aum.max(1))
            .ok_or(VaultError::MathError)?
    };

    // Update user rewards (if you track them), then mint LP
    update_user_rewards(pool_state, user_state)?;

    let cpi_ctx_mint = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        MintTo {
            mint: ctx.accounts.lp_token_mint.to_account_info(),
            to: ctx.accounts.user_lp_token_account.to_account_info(),
            authority: pool_state.to_account_info(),
        },
    );
    token::mint_to(cpi_ctx_mint.with_signer(&[]), lp_to_mint)?;

    // Update user's record of how many LP tokens they hold
    user_state.owner = ctx.accounts.user.key();
    user_state.lp_token_balance = user_state
        .lp_token_balance
        .checked_add(lp_to_mint)
        .ok_or(VaultError::MathError)?;

    msg!("Deposit successful. Minted {} LP tokens.", lp_to_mint);
    Ok(())
}
