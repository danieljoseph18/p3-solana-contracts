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
    #[account(
        mut,
        seeds = [b"pool-state".as_ref()],
        bump
    )]
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
    msg!("Starting deposit of {} tokens", token_amount);

    // For readability
    let pool_state = &mut ctx.accounts.pool_state;
    let user_state = &mut ctx.accounts.user_state;

    // If depositing SOL, fetch latest price from Chainlink.
    if ctx.accounts.vault_account.key() == pool_state.sol_vault {
        msg!("Depositing SOL, fetching latest Chainlink price");
        let round = chainlink::latest_round_data(
            ctx.accounts.chainlink_program.to_account_info(),
            ctx.accounts.chainlink_feed.to_account_info(),
        )?;
        // Update stored SOL price (8 decimals from Chainlink)
        pool_state.sol_usd_price = round.answer;
        msg!("Updated SOL/USD price to {} (8 dec)", round.answer);
    }

    msg!("Transferring {} tokens to vault", token_amount);
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
    msg!("Token transfer successful");

    // Determine how many tokens in USD were deposited (6 decimals).
    // Also update the pool's recorded total (sol_deposited / usdc_deposited).
    let deposit_usd = if ctx.accounts.vault_account.key() == pool_state.sol_vault {
        msg!("Processing SOL deposit");
        // Increase total SOL (9 decimals)
        pool_state.sol_deposited = pool_state
            .sol_deposited
            .checked_add(token_amount)
            .ok_or(VaultError::MathError)?;
        msg!(
            "Updated pool SOL balance to {} (9 dec)",
            pool_state.sol_deposited
        );

        // Convert SOL to USD (returns USD with 6 decimals)
        get_sol_usd_value(token_amount, pool_state.sol_usd_price)?
    } else if ctx.accounts.vault_account.key() == pool_state.usdc_vault {
        msg!("Processing USDC deposit");
        // Increase total USDC (6 decimals)
        pool_state.usdc_deposited = pool_state
            .usdc_deposited
            .checked_add(token_amount)
            .ok_or(VaultError::MathError)?;
        msg!(
            "Updated pool USDC balance to {} (6 dec)",
            pool_state.usdc_deposited
        );

        // USDC already has 6 decimals, matching our USD representation
        token_amount
    } else {
        return err!(VaultError::InvalidTokenMint);
    };
    msg!("Deposit value in USD: {} (6 dec)", deposit_usd);

    // Now compute the *current* AUM (in USD with 6 decimals) based on updated totals.
    msg!("Computing current AUM");
    // 1) Convert total SOL to USD (6 decimals), 2) Add total USDC (6 decimals)
    let total_sol_usd = get_sol_usd_value(pool_state.sol_deposited, pool_state.sol_usd_price)?;
    msg!("Total SOL value in USD: {} (6 dec)", total_sol_usd);
    let current_aum = total_sol_usd
        .checked_add(pool_state.usdc_deposited)
        .ok_or(VaultError::MathError)?;
    msg!("Current total AUM: {} (6 dec)", current_aum);

    // Figure out how many LP tokens to mint:
    msg!("Calculating LP tokens to mint");
    //   if first deposit, deposit_usd tokens (6 decimals)
    //   else deposit_usd * (LP supply / AUM) -> maintains 6 decimals due to ratio
    let lp_supply = ctx.accounts.lp_token_mint.supply;
    msg!("Current LP token supply: {}", lp_supply);

    let lp_to_mint = if lp_supply == 0 {
        msg!("First deposit - LP tokens will match USD value");
        // For first deposit, LP tokens match USD value (6 decimals)
        deposit_usd
    } else {
        msg!("Calculating proportional LP tokens");
        // For subsequent deposits:
        // deposit_usd (6 dec) * lp_supply / current_aum (6 dec) = result with 6 decimals
        deposit_usd
            .checked_mul(lp_supply)
            .ok_or(VaultError::MathError)?
            .checked_div(current_aum.max(1))
            .ok_or(VaultError::MathError)?
    };
    msg!("Will mint {} LP tokens (6 dec)", lp_to_mint);

    // Update user rewards (if you track them), then mint LP
    msg!("Updating user rewards before minting");
    update_user_rewards(pool_state, user_state)?;

    // Mint LP tokens (which maintain 6 decimals like USD)
    msg!("Minting LP tokens to user");
    let cpi_ctx_mint = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        MintTo {
            mint: ctx.accounts.lp_token_mint.to_account_info(),
            to: ctx.accounts.user_lp_token_account.to_account_info(),
            authority: pool_state.to_account_info(),
        },
    );
    token::mint_to(
        cpi_ctx_mint.with_signer(&[&[b"pool-state".as_ref(), &[ctx.bumps.pool_state]]]),
        lp_to_mint,
    )?;

    // Update user's record of how many LP tokens they hold (6 decimals)
    user_state.owner = ctx.accounts.user.key();
    user_state.lp_token_balance = user_state
        .lp_token_balance
        .checked_add(lp_to_mint)
        .ok_or(VaultError::MathError)?;
    msg!(
        "Updated user's LP token balance to {} (6 dec)",
        user_state.lp_token_balance
    );

    msg!(
        "Deposit successful. Minted {} LP tokens (6 decimals).",
        lp_to_mint
    );
    Ok(())
}
