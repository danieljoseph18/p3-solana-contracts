use anchor_lang::prelude::*;

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

    /// USDC vault account (USDC uses 6 decimals, so 1 USDC = 1_000_000)
    pub usdc_vault: Pubkey,

    /// LP token mint
    pub lp_token_mint: Pubkey,

    /// How many SOL tokens are currently deposited in total (9 decimals, 1 SOL = 1_000_000_000)
    pub sol_deposited: u64,

    /// How many USDC tokens are currently deposited in total (6 decimals, 1 USDC = 1_000_000)
    pub usdc_deposited: u64,

    /// USDC earned per second per LP token (6 decimals)
    pub tokens_per_interval: u64,

    /// Timestamp when current reward distribution started
    pub reward_start_time: u64,

    /// Timestamp when rewards stop accruing (start + 604800)
    pub reward_end_time: u64,

    /// Vault holding USDC rewards
    pub usdc_reward_vault: Pubkey,

    /// Current SOL/USD price from Chainlink (8 decimals from feed)
    pub sol_usd_price: i128,

    // -----------------------------------------------
    // New fields to ensure we never exceed the deposited rewards
    // -----------------------------------------------
    /// How many USDC tokens the admin deposited for this reward period (6 decimals)
    pub total_rewards_deposited: u64,

    /// How many USDC have actually been claimed by users so far (6 decimals)
    pub total_rewards_claimed: u64,
}

impl PoolState {
    /// Adjust this if you add or remove fields
    pub const LEN: usize = 32  // admin
        + 32                  // sol_vault
        + 32                  // usdc_vault
        + 32                  // lp_token_mint
        + 8                   // sol_deposited
        + 8                   // usdc_deposited
        + 8                   // tokens_per_interval
        + 8                   // reward_start_time
        + 8                   // reward_end_time
        + 32                  // usdc_reward_vault
        + 16                  // sol_usd_price (i128)
        + 8                   // total_rewards_deposited
        + 8; // total_rewards_claimed
}

/// UserState stores user-specific info (in practice often combined into a single PDA).
#[account]
pub struct UserState {
    /// User pubkey
    pub owner: Pubkey,

    /// User's LP token balance (tracked within the program, not minted supply)
    pub lp_token_balance: u64,

    /// Last time user claimed (or had rewards updated)
    pub last_claim_timestamp: u64,

    /// Accumulated USDC rewards that have not yet been claimed
    pub pending_rewards: u64,
}

impl UserState {
    pub const LEN: usize = 32 // owner
        + 8  // lp_token_balance
        + 8  // last_claim_timestamp
        + 8; // pending_rewards
}

// -----------------------------------------------
// Chainlink conversion helpers
// -----------------------------------------------

/// Helper function for SOL -> USD conversions using the `sol_usd_price` from Chainlink.
///
/// Input:
///   - sol_amount: Amount of SOL with 9 decimals (1 SOL = 1_000_000_000)
///   - sol_usd_price: Chainlink price with 8 decimals
/// Output:
///   - USD value with 6 decimals (1 USD = 1_000_000)
pub fn get_sol_usd_value(sol_amount: u64, sol_usd_price: i128) -> Result<u64> {
    msg!(
        "Converting SOL to USD | SOL amount: {} (9 dec), SOL/USD price: {} (8 dec)",
        sol_amount,
        sol_usd_price
    );

    // Convert SOL to USD with proper decimal handling:
    // 1. Multiply SOL (9 decimals) by price (8 decimals)
    // 2. Divide by 10^8 (Chainlink decimals) to get to raw USD
    // 3. Divide by 1000 (9 - 6 = 3) to convert to 6 decimal USD
    let usd = (sol_amount as u128)
        .checked_mul(sol_usd_price as u128)
        .unwrap_or(0)
        .checked_div(100_000_000) // Remove Chainlink's 8 decimals
        .unwrap_or(0)
        .checked_div(1000) // Convert from 9 to 6 decimals
        .unwrap_or(0);

    msg!("Conversion result: {} USD (6 dec)", usd);
    Ok(usd as u64)
}

/// Helper function for USD -> SOL conversions using the `sol_usd_price` from Chainlink.
///
/// Input:
///   - usd_value: USD amount with 6 decimals (1 USD = 1_000_000)
///   - sol_usd_price: Chainlink price with 8 decimals
/// Output:
///   - SOL amount with 9 decimals (1 SOL = 1_000_000_000)
pub fn get_sol_amount_from_usd(usd_value: u64, sol_usd_price: i128) -> Result<u64> {
    msg!(
        "Converting USD to SOL | USD amount: {} (6 dec), SOL/USD price: {} (8 dec)",
        usd_value,
        sol_usd_price
    );

    let sol = (usd_value as u128)
        .checked_mul(100_000_000) // Add Chainlink's 8 decimals
        .unwrap_or(0)
        .checked_mul(1000) // Convert from 6 to 9 decimals
        .unwrap_or(0)
        .checked_div(sol_usd_price as u128)
        .unwrap_or(0);

    msg!("Conversion result: {} SOL (9 dec)", sol);
    Ok(sol as u64)
}
