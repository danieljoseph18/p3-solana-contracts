use crate::{errors::VaultError, instructions::helpers::*, state::*};
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, MintTo, Token, TokenAccount, Transfer};

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

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handle_deposit(ctx: Context<Deposit>, token_amount: u64) -> Result<()> {
    // Capture values we need before mutable borrow
    let sol_vault = ctx.accounts.pool_state.sol_vault;
    let usdc_vault = ctx.accounts.pool_state.usdc_vault;
    let current_aum = ctx.accounts.pool_state.aum_usd;

    let user_state = &mut ctx.accounts.user_state;

    // Transfer tokens from user to vault
    let cpi_ctx = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        Transfer {
            from: ctx.accounts.user_token_account.to_account_info(),
            to: ctx.accounts.vault_account.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        },
    );
    token::transfer(cpi_ctx, token_amount)?;

    // Convert deposit to USD (1:1 if USDC, or via get_sol_usd_value if SOL)
    let deposit_usd = if ctx.accounts.vault_account.mint == sol_vault {
        get_sol_usd_value(token_amount, ctx.accounts.pool_state.sol_usd_price)?
    } else if ctx.accounts.vault_account.mint == usdc_vault {
        token_amount
    } else {
        return err!(VaultError::InvalidTokenMint);
    };

    let lp_supply = ctx.accounts.lp_token_mint.supply;
    let lp_to_mint = if lp_supply == 0 {
        deposit_usd
    } else {
        deposit_usd
            .checked_mul(lp_supply)
            .ok_or_else(|| error!(VaultError::MathError))?
            .checked_div(current_aum.max(1))
            .ok_or_else(|| error!(VaultError::MathError))?
    };

    let pool_state = &mut ctx.accounts.pool_state;
    update_user_rewards(pool_state, user_state)?;

    // Mint LP tokens to user
    let cpi_ctx_mint = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        MintTo {
            mint: ctx.accounts.lp_token_mint.to_account_info(),
            to: ctx.accounts.user_lp_token_account.to_account_info(),
            authority: pool_state.to_account_info(),
        },
    );
    token::mint_to(cpi_ctx_mint.with_signer(&[]), lp_to_mint)?;

    // Update user & pool state
    user_state.owner = ctx.accounts.user.key();
    user_state.lp_token_balance = user_state
        .lp_token_balance
        .checked_add(lp_to_mint)
        .ok_or_else(|| error!(VaultError::MathError))?;

    pool_state.aum_usd = pool_state
        .aum_usd
        .checked_add(deposit_usd)
        .ok_or_else(|| error!(VaultError::MathError))?;

    msg!("Deposit successful. Minted {} LP tokens.", lp_to_mint);

    Ok(())
}
