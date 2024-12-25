use anchor_lang::prelude::*;

// We do *not* declare an ID here again. Only one declare_id! in lib.rs.

// -----------------------------------------------
// Context structs for Chainlink usage
// -----------------------------------------------

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = admin, space = 8 + PoolState::LEN)]
    pub pool_state: Account<'info, PoolState>,

    #[account(mut)]
    pub admin: Signer<'info>,

    /// CHECK: This is not dangerous if you're verifying usage in your logic
    pub sol_vault: AccountInfo<'info>,

    /// CHECK: Same as above, ensure usage is validated
    pub usdc_vault: AccountInfo<'info>,

    /// CHECK: Similarly ensure it's your mint
    pub lp_token_mint: AccountInfo<'info>,

    /// CHECK: Program's reward vault
    pub usdc_reward_vault: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateSolUsdPrice<'info> {
    #[account(mut)]
    pub pool_state: Account<'info, PoolState>,

    /// CHECK: This is the Chainlink program's address
    pub chainlink_program: AccountInfo<'info>,

    /// CHECK: This is the Chainlink feed account
    pub chainlink_feed: AccountInfo<'info>,
}

// -----------------------------------------------
// Data structures for the pool
// -----------------------------------------------

/// PoolState holds global info about the liquidity pool.
#[account]
pub struct PoolState {
    /// Admin authority who can withdraw funds and set rewards
    pub admin: Pubkey,

    /// SOL vault account (token account for wrapped SOL or special handling)
    pub sol_vault: Pubkey,

    /// USDC vault account
    pub usdc_vault: Pubkey,

    /// LP token mint
    pub lp_token_mint: Pubkey,

    /// Total value locked in USD
    pub aum_usd: u64,

    /// USDC earned per second per LP token
    pub tokens_per_interval: u64,

    /// Timestamp when current reward distribution started
    pub reward_start_time: u64,

    /// Timestamp when rewards stop accruing (start + 604800)
    pub reward_end_time: u64,

    /// Vault holding USDC rewards
    pub usdc_reward_vault: Pubkey,

    /// Current SOL/USD price from Chainlink
    pub sol_usd_price: i128,
}

impl PoolState {
    // Adjust LEN if your final layout changes
    pub const LEN: usize = 32  // admin
        + 32  // sol_vault
        + 32  // usdc_vault
        + 32  // lp_token_mint
        + 8   // aum_usd
        + 8   // tokens_per_interval
        + 8   // reward_start_time
        + 8   // reward_end_time
        + 32  // usdc_reward_vault
        + 16; // sol_usd_price
}

/// UserState stores user-specific info (in practice often combined into a single PDA).
#[account]
pub struct UserState {
    /// User pubkey
    pub owner: Pubkey,
    /// User's LP token balance (tracked within the program, not minted supply)
    pub lp_token_balance: u64,
    /// Last time user claimed rewards
    pub last_claim_timestamp: u64,
}

// -----------------------------------------------
// Chainlink conversion helpers
// -----------------------------------------------

/// Helper function for SOL -> USD conversions using the `sol_usd_price` from Chainlink.
pub fn get_sol_usd_value(sol_amount: u64, sol_usd_price: i128) -> Result<u64> {
    // Example: Chainlink's price feed might be $20 with an 8-decimal feed
    // so round.answer = 2,000,000,000 for $20 * 100,000,000
    // Hence dividing by 100,000,000 to get back to "raw" USD value.
    let usd = (sol_amount as u128)
        .checked_mul(sol_usd_price as u128)
        .unwrap_or(0)
        .checked_div(100_000_000)
        .unwrap_or(0);
    Ok(usd as u64)
}

/// Helper function for USD -> SOL conversions using the `sol_usd_price` from Chainlink.
pub fn get_sol_amount_from_usd(usd_value: u64, sol_usd_price: i128) -> Result<u64> {
    let sol = (usd_value as u128)
        .checked_mul(100_000_000)
        .unwrap_or(0)
        .checked_div(sol_usd_price as u128)
        .unwrap_or(0);
    Ok(sol as u64)
}
