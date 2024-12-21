use anchor_lang::{prelude::*, solana_program::account_info::AccountInfo};
use anchor_spl::{
    associated_token::AssociatedToken,
    token_2022::{Token2022, ID as TOKEN_2022_ID},
    token_interface::{Mint, TokenAccount},
};
use pyth_sdk_solana::{load_price_feed_from_account_info, Price, PriceFeed};

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

const STALENESS_THRESHOLD: u64 = 60; // 60 seconds staleness threshold

// Helper function to get SOL price in USD (6 decimals)
fn get_sol_price(pyth_price_info: &AccountInfo) -> Result<u64> {
    let price_feed: PriceFeed =
        load_price_feed_from_account_info(unsafe { std::mem::transmute(pyth_price_info) })
            .map_err(|_| error!(ErrorCode::InvalidPythOracle))?;

    let current_timestamp = Clock::get()?.unix_timestamp;
    let price: Price = price_feed
        .get_price_no_older_than(current_timestamp, STALENESS_THRESHOLD)
        .ok_or(error!(ErrorCode::StaleOracle))?;

    // Check confidence interval (1% of price)
    let conf_interval: f64 = (price.conf as f64) / (price.price as f64).abs();
    require!(conf_interval <= 0.01, ErrorCode::PriceConfidenceTooWide);

    // Convert price to 6 decimals (USDC standard)
    let sol_price = ((price.price as f64) * 10f64.powi(6 - price.expo)) as u64;
    require!(sol_price > 0, ErrorCode::InvalidOraclePrice);

    Ok(sol_price)
}

// Helper function to get USDC price in USD (6 decimals)
fn get_usdc_price(pyth_price_info: &AccountInfo) -> Result<u64> {
    let price_feed: PriceFeed =
        load_price_feed_from_account_info(unsafe { std::mem::transmute(pyth_price_info) })
            .map_err(|_| error!(ErrorCode::InvalidPythOracle))?;

    let current_timestamp = Clock::get()?.unix_timestamp;
    let price = price_feed
        .get_price_no_older_than(current_timestamp, STALENESS_THRESHOLD)
        .ok_or(error!(ErrorCode::StaleOracle))?;

    // Check confidence interval (0.1% of price for USDC)
    let conf_interval: f64 = (price.conf as f64) / (price.price as f64).abs();
    require!(conf_interval <= 0.001, ErrorCode::PriceConfidenceTooWide);

    // Convert price to 6 decimals (USDC standard)
    let usdc_price: u64 = ((price.price as f64) * 10f64.powi(6 - price.expo)) as u64;
    require!(usdc_price > 0, ErrorCode::InvalidOraclePrice);

    // Verify USDC hasn't significantly depegged
    require!(
        usdc_price >= 999_000 && usdc_price <= 1_001_000, // Allow 0.1% deviation
        ErrorCode::UsdcDepegged
    );

    Ok(usdc_price)
}

// Internal helper function for claiming rewards
fn claim_rewards_internal<'info>(
    pool_state: &Account<'info, PoolState>,
    user_state: &mut Account<'info, UserState>,
    usdc_reward_vault: &InterfaceAccount<'info, TokenAccount>,
    user_usdc_account: &InterfaceAccount<'info, TokenAccount>,
    token_program: &Program<'info, Token2022>,
) -> Result<()> {
    // Calculate elapsed time since last claim
    let current_time = Clock::get()?.unix_timestamp;
    let last_claim = user_state.last_claim_timestamp;
    let reward_end = pool_state.reward_end_time;

    let time_elapsed = std::cmp::min(current_time - last_claim, reward_end - last_claim);

    // Skip if no time has elapsed or rewards have not started
    if time_elapsed <= 0 || current_time < pool_state.reward_start_time {
        return Ok(());
    }

    // Calculate rewards
    let rewards = (user_state.lp_token_balance as u128)
        .checked_mul(pool_state.tokens_per_interval as u128)
        .unwrap()
        .checked_mul(time_elapsed as u128)
        .unwrap()
        .checked_div(1_000_000_000) // Scale down by 1e9 to maintain precision
        .unwrap() as u64;

    if rewards > 0 {
        // Transfer USDC rewards to user
        anchor_spl::token_interface::transfer(
            CpiContext::new_with_signer(
                token_program.to_account_info(),
                anchor_spl::token_interface::Transfer {
                    from: usdc_reward_vault.to_account_info(),
                    to: user_usdc_account.to_account_info(),
                    authority: pool_state.to_account_info(),
                },
                &[&[b"pool"]],
            ),
            rewards,
        )?;
    }

    // Update last claim time
    user_state.last_claim_timestamp = current_time;

    Ok(())
}

#[program]
pub mod solana_liquidity_pool {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let pool_state = &mut ctx.accounts.pool_state;
        pool_state.admin = ctx.accounts.admin.key();
        pool_state.sol_vault = ctx.accounts.sol_vault.key();
        pool_state.usdc_vault = ctx.accounts.usdc_vault.key();
        pool_state.lp_token_mint = ctx.accounts.lp_token_mint.key();
        pool_state.aum_usd = 0;
        pool_state.tokens_per_interval = 0;
        pool_state.reward_start_time = 0;
        pool_state.reward_end_time = 0;
        pool_state.usdc_reward_vault = ctx.accounts.usdc_reward_vault.key();
        pool_state.paused = false;
        Ok(())
    }

    pub fn set_pause(ctx: Context<AdminOnly>, paused: bool) -> Result<()> {
        let pool_state = &mut ctx.accounts.pool_state;

        // Verify admin
        require!(
            ctx.accounts.admin.key() == pool_state.admin,
            ErrorCode::NotAdmin
        );

        pool_state.paused = paused;
        Ok(())
    }

    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        let pool_state = &ctx.accounts.pool_state;

        // Check if paused
        require!(!pool_state.paused, ErrorCode::ProgramPaused);

        // If user has existing LP tokens, claim rewards before updating balance
        if ctx.accounts.user_state.lp_token_balance > 0 {
            claim_rewards_internal(
                &ctx.accounts.pool_state,
                &mut ctx.accounts.user_state,
                &ctx.accounts.usdc_reward_vault,
                &ctx.accounts.user_usdc_account,
                &ctx.accounts.token_program,
            )?;
        }

        // Calculate USD value of deposit
        let usd_value = if ctx.accounts.token_mint.key() == ctx.accounts.sol_mint.key() {
            let sol_price = get_sol_price(&ctx.accounts.pyth_sol_price)?;
            (amount as u128)
                .checked_mul(sol_price as u128)
                .unwrap()
                .checked_div(1_000_000_000) // Convert from SOL decimals (9) to USDC decimals (6)
                .unwrap() as u64
        } else {
            // Verify USDC price
            let usdc_price = get_usdc_price(&ctx.accounts.pyth_usdc_price)?;
            (amount as u128)
                .checked_mul(usdc_price as u128)
                .unwrap()
                .checked_div(1_000_000) // Scale by USDC decimals
                .unwrap() as u64
        };

        // Calculate LP tokens to mint
        let total_supply = ctx.accounts.lp_token_mint.supply;
        let lp_tokens_to_mint = if total_supply == 0 {
            usd_value // Initial price of $1
        } else {
            // LP token price = AUM / total supply
            (usd_value as u128)
                .checked_mul(total_supply as u128)
                .unwrap()
                .checked_div(pool_state.aum_usd as u128)
                .unwrap() as u64
        };

        // Update AUM
        let mut pool_state = ctx.accounts.pool_state.clone();
        pool_state.aum_usd = pool_state.aum_usd.checked_add(usd_value).unwrap();

        // Transfer tokens to vault
        anchor_spl::token_interface::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                anchor_spl::token_interface::Transfer {
                    from: ctx.accounts.user_token_account.to_account_info(),
                    to: ctx.accounts.token_vault.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            amount,
        )?;

        // Mint LP tokens
        anchor_spl::token_interface::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                anchor_spl::token_interface::MintTo {
                    mint: ctx.accounts.lp_token_mint.to_account_info(),
                    to: ctx.accounts.user_lp_token_account.to_account_info(),
                    authority: pool_state.to_account_info(),
                },
                &[&[b"pool", &[ctx.bumps.pool_state]]],
            ),
            lp_tokens_to_mint,
        )?;

        // Update user state
        let mut user_state = ctx.accounts.user_state.clone();
        user_state.lp_token_balance = user_state
            .lp_token_balance
            .checked_add(lp_tokens_to_mint)
            .unwrap();

        Ok(())
    }

    pub fn withdraw(ctx: Context<Withdraw>, lp_token_amount: u64) -> Result<()> {
        let pool_state = &ctx.accounts.pool_state;

        // Check if paused
        require!(!pool_state.paused, ErrorCode::ProgramPaused);

        // If user has LP tokens, claim rewards before updating balance
        if ctx.accounts.user_state.lp_token_balance > 0 {
            claim_rewards_internal(
                &ctx.accounts.pool_state,
                &mut ctx.accounts.user_state,
                &ctx.accounts.usdc_reward_vault,
                &ctx.accounts.user_usdc_account,
                &ctx.accounts.token_program,
            )?;
        }

        // Verify user has enough LP tokens
        require!(
            ctx.accounts.user_state.lp_token_balance >= lp_token_amount,
            ErrorCode::InsufficientLPTokens
        );

        // Calculate withdrawal value in USD
        let total_supply = ctx.accounts.lp_token_mint.supply;
        let withdrawal_usd_value = (lp_token_amount as u128)
            .checked_mul(pool_state.aum_usd as u128)
            .unwrap()
            .checked_div(total_supply as u128)
            .unwrap() as u64;

        // Calculate token amount to return
        let token_amount = if ctx.accounts.token_mint.key() == ctx.accounts.sol_mint.key() {
            let sol_price = get_sol_price(&ctx.accounts.pyth_sol_price)?;
            (withdrawal_usd_value as u128)
                .checked_mul(1_000_000_000) // Convert to SOL decimals
                .unwrap()
                .checked_div(sol_price as u128)
                .unwrap() as u64
        } else {
            withdrawal_usd_value // USDC is 1:1 with USD
        };

        // Update AUM
        let mut pool_state = ctx.accounts.pool_state.clone();
        pool_state.aum_usd = pool_state
            .aum_usd
            .checked_sub(withdrawal_usd_value)
            .unwrap();

        // Burn LP tokens
        anchor_spl::token_interface::burn(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                anchor_spl::token_interface::Burn {
                    mint: ctx.accounts.lp_token_mint.to_account_info(),
                    from: ctx.accounts.user_lp_token_account.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            lp_token_amount,
        )?;

        // Transfer tokens from vault to user
        anchor_spl::token_interface::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                anchor_spl::token_interface::Transfer {
                    from: ctx.accounts.token_vault.to_account_info(),
                    to: ctx.accounts.user_token_account.to_account_info(),
                    authority: pool_state.to_account_info(),
                },
                &[&[b"pool", &[ctx.bumps.pool_state]]],
            ),
            token_amount,
        )?;

        // Update user state
        let mut user_state = ctx.accounts.user_state.clone();
        user_state.lp_token_balance = user_state
            .lp_token_balance
            .checked_sub(lp_token_amount)
            .unwrap();

        Ok(())
    }

    pub fn admin_withdraw(ctx: Context<AdminOperation>, amount: u64) -> Result<()> {
        let pool_state = &mut ctx.accounts.pool_state;

        // Verify admin
        require!(
            ctx.accounts.admin.key() == pool_state.admin,
            ErrorCode::NotAdmin
        );

        // Calculate USD value
        let usd_value = if ctx.accounts.token_mint.key() == ctx.accounts.sol_mint.key() {
            let sol_price = get_sol_price(&ctx.accounts.pyth_sol_price)?;
            (amount as u128)
                .checked_mul(sol_price as u128)
                .unwrap()
                .checked_div(1_000_000_000)
                .unwrap() as u64
        } else {
            amount // USDC is 1:1 with USD
        };

        // Update AUM
        pool_state.aum_usd = pool_state.aum_usd.checked_sub(usd_value).unwrap();

        // Transfer tokens from vault to admin
        anchor_spl::token_interface::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                anchor_spl::token_interface::Transfer {
                    from: ctx.accounts.token_vault.to_account_info(),
                    to: ctx.accounts.admin_token_account.to_account_info(),
                    authority: pool_state.to_account_info(),
                },
                &[&[b"pool", &[ctx.bumps.pool_state]]],
            ),
            amount,
        )?;

        Ok(())
    }

    pub fn admin_deposit(ctx: Context<AdminOperation>, amount: u64) -> Result<()> {
        let pool_state = &mut ctx.accounts.pool_state;

        // Verify admin
        require!(
            ctx.accounts.admin.key() == pool_state.admin,
            ErrorCode::NotAdmin
        );

        // Calculate USD value
        let usd_value = if ctx.accounts.token_mint.key() == ctx.accounts.sol_mint.key() {
            let sol_price = get_sol_price(&ctx.accounts.pyth_sol_price)?;
            (amount as u128)
                .checked_mul(sol_price as u128)
                .unwrap()
                .checked_div(1_000_000_000)
                .unwrap() as u64
        } else {
            amount // USDC is 1:1 with USD
        };

        // Update AUM
        pool_state.aum_usd = pool_state.aum_usd.checked_add(usd_value).unwrap();

        // Transfer tokens from admin to vault
        anchor_spl::token_interface::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                anchor_spl::token_interface::Transfer {
                    from: ctx.accounts.admin_token_account.to_account_info(),
                    to: ctx.accounts.token_vault.to_account_info(),
                    authority: ctx.accounts.admin.to_account_info(),
                },
            ),
            amount,
        )?;

        Ok(())
    }

    pub fn start_rewards(
        ctx: Context<StartRewards>,
        usdc_amount: u64,
        tokens_per_interval: u64,
    ) -> Result<()> {
        let pool_state = &mut ctx.accounts.pool_state;

        // Verify admin
        require!(
            ctx.accounts.admin.key() == pool_state.admin,
            ErrorCode::NotAdmin
        );

        // Transfer USDC rewards to vault
        anchor_spl::token_interface::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                anchor_spl::token_interface::Transfer {
                    from: ctx.accounts.admin_usdc_account.to_account_info(),
                    to: ctx.accounts.usdc_reward_vault.to_account_info(),
                    authority: ctx.accounts.admin.to_account_info(),
                },
            ),
            usdc_amount,
        )?;

        // Set reward parameters
        pool_state.tokens_per_interval = tokens_per_interval;
        pool_state.reward_start_time = Clock::get()?.unix_timestamp;
        pool_state.reward_end_time = pool_state.reward_start_time + 604800; // One week

        Ok(())
    }
}

#[account]
#[derive(Default)]
pub struct PoolState {
    pub admin: Pubkey,             // 32
    pub sol_vault: Pubkey,         // 32
    pub usdc_vault: Pubkey,        // 32
    pub lp_token_mint: Pubkey,     // 32
    pub aum_usd: u64,              // 8
    pub tokens_per_interval: u64,  // 8
    pub reward_start_time: i64,    // 8
    pub reward_end_time: i64,      // 8
    pub usdc_reward_vault: Pubkey, // 32
    pub paused: bool,              // 1
}

#[account]
#[derive(Default)]
pub struct UserState {
    pub owner: Pubkey,             // 32
    pub lp_token_balance: u64,     // 8
    pub last_claim_timestamp: i64, // 8
}

impl UserState {
    pub const SIZE: usize = 8 + // discriminator
                           32 + // owner (Pubkey)
                           8 +  // lp_token_balance
                           8; // last_claim_timestamp
}

#[error_code]
pub enum ErrorCode {
    #[msg("Insufficient LP tokens for withdrawal")]
    InsufficientLPTokens,
    #[msg("Only admin can perform this operation")]
    NotAdmin,
    #[msg("Invalid Pyth oracle account")]
    InvalidPythOracle,
    #[msg("Oracle price is stale")]
    StaleOracle,
    #[msg("Invalid oracle price")]
    InvalidOraclePrice,
    #[msg("USDC has depegged beyond acceptable threshold")]
    UsdcDepegged,
    #[msg("Price confidence interval too wide")]
    PriceConfidenceTooWide,
    #[msg("Program is paused")]
    ProgramPaused,
}

#[derive(Accounts)]
#[instruction(bump: u8)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        init,
        payer = admin,
        space = 8 + std::mem::size_of::<PoolState>(),
        seeds = [b"pool"],
        bump
    )]
    pub pool_state: Account<'info, PoolState>,

    #[account(
        init,
        payer = admin,
        token::mint = sol_mint,
        token::authority = pool_state,
        token::token_program = token_program,
    )]
    pub sol_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        init,
        payer = admin,
        token::mint = usdc_mint,
        token::authority = pool_state,
        token::token_program = token_program,
    )]
    pub usdc_vault: InterfaceAccount<'info, TokenAccount>,

    pub sol_mint: InterfaceAccount<'info, Mint>,
    pub usdc_mint: InterfaceAccount<'info, Mint>,

    #[account(
        init,
        payer = admin,
        mint::decimals = 9,
        mint::authority = pool_state,
        mint::token_program = token_program,
    )]
    pub lp_token_mint: InterfaceAccount<'info, Mint>,

    #[account(
        init,
        payer = admin,
        token::mint = usdc_mint,
        token::authority = pool_state,
        token::token_program = token_program,
    )]
    pub usdc_reward_vault: InterfaceAccount<'info, TokenAccount>,

    pub system_program: Program<'info, System>,
    #[account(constraint = token_program.key() == TOKEN_2022_ID)]
    pub token_program: Program<'info, Token2022>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
#[instruction(amount: u64)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"pool"],
        bump,
    )]
    pub pool_state: Account<'info, PoolState>,

    #[account(
        init_if_needed,
        payer = user,
        space = 8 + std::mem::size_of::<UserState>(),
        seeds = [b"user_state", user.key().as_ref()],
        bump
    )]
    pub user_state: Account<'info, UserState>,

    pub token_mint: InterfaceAccount<'info, Mint>,
    pub sol_mint: InterfaceAccount<'info, Mint>,
    pub usdc_mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        constraint = token_vault.mint == token_mint.key(),
        constraint = token_vault.owner == pool_state.key()
    )]
    pub token_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        constraint = user_token_account.mint == token_mint.key(),
        constraint = user_token_account.owner == user.key()
    )]
    pub user_token_account: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        constraint = lp_token_mint.key() == pool_state.lp_token_mint
    )]
    pub lp_token_mint: InterfaceAccount<'info, Mint>,

    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = lp_token_mint,
        associated_token::authority = user,
        associated_token::token_program = token_program,
    )]
    pub user_lp_token_account: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        constraint = usdc_reward_vault.mint == usdc_mint.key(),
        constraint = usdc_reward_vault.owner == pool_state.key()
    )]
    pub usdc_reward_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        constraint = user_usdc_account.mint == usdc_mint.key(),
        constraint = user_usdc_account.owner == user.key()
    )]
    pub user_usdc_account: InterfaceAccount<'info, TokenAccount>,

    /// CHECK: Validated in instruction
    pub pyth_sol_price: AccountInfo<'info>,
    /// CHECK: Validated in instruction
    pub pyth_usdc_price: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
    #[account(constraint = token_program.key() == TOKEN_2022_ID)]
    pub token_program: Program<'info, Token2022>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
#[instruction(lp_token_amount: u64)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"pool"],
        bump
    )]
    pub pool_state: Account<'info, PoolState>,

    #[account(
        mut,
        seeds = [b"user_state", user.key().as_ref()],
        bump
    )]
    pub user_state: Account<'info, UserState>,

    pub token_mint: InterfaceAccount<'info, Mint>,
    pub sol_mint: InterfaceAccount<'info, Mint>,

    #[account(mut)]
    pub token_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(mut)]
    pub user_token_account: InterfaceAccount<'info, TokenAccount>,

    #[account(mut)]
    pub lp_token_mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        associated_token::mint = lp_token_mint,
        associated_token::authority = user,
        associated_token::token_program = token_program,
    )]
    pub user_lp_token_account: InterfaceAccount<'info, TokenAccount>,

    pub system_program: Program<'info, System>,
    #[account(constraint = token_program.key() == TOKEN_2022_ID)]
    pub token_program: Program<'info, Token2022>,

    /// CHECK: Verified in get_sol_price
    pub pyth_sol_price: AccountInfo<'info>,

    /// CHECK: Verified in get_usdc_price
    pub pyth_usdc_price: AccountInfo<'info>,

    #[account(mut)]
    pub usdc_reward_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(mut)]
    pub user_usdc_account: InterfaceAccount<'info, TokenAccount>,

    pub usdc_mint: InterfaceAccount<'info, Mint>,
}

#[derive(Accounts)]
#[instruction(amount: u64)]
pub struct AdminOperation<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [b"pool"],
        bump
    )]
    pub pool_state: Account<'info, PoolState>,

    pub token_mint: InterfaceAccount<'info, Mint>,
    pub sol_mint: InterfaceAccount<'info, Mint>,

    #[account(mut)]
    pub token_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(mut)]
    pub admin_token_account: InterfaceAccount<'info, TokenAccount>,

    #[account(constraint = token_program.key() == TOKEN_2022_ID)]
    pub token_program: Program<'info, Token2022>,

    /// CHECK: Verified in get_sol_price
    pub pyth_sol_price: AccountInfo<'info>,

    /// CHECK: Verified in get_usdc_price
    pub pyth_usdc_price: AccountInfo<'info>,
}

#[derive(Accounts)]
#[instruction(usdc_amount: u64, tokens_per_interval: u64)]
pub struct StartRewards<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [b"pool"],
        bump
    )]
    pub pool_state: Account<'info, PoolState>,

    #[account(mut)]
    pub admin_usdc_account: InterfaceAccount<'info, TokenAccount>,

    #[account(mut)]
    pub usdc_reward_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(constraint = token_program.key() == TOKEN_2022_ID)]
    pub token_program: Program<'info, Token2022>,
}

#[derive(Accounts)]
#[instruction(paused: bool)]
pub struct AdminOnly<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [b"pool"],
        bump
    )]
    pub pool_state: Account<'info, PoolState>,
}
