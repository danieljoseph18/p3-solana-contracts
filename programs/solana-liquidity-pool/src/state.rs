use anchor_lang::prelude::*;

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

/// Helper function stubs for SOL <-> USD conversions (replace with real oracles).
pub fn get_sol_usd_value(sol_amount: u64) -> Result<u64> {
    // Replace with an oracle or some mock logic
    // Example: 1 SOL = $20
    Ok(sol_amount * 20)
}

pub fn get_sol_amount_from_usd(usd_value: u64) -> Result<u64> {
    // Example: 1 SOL = $20
    Ok(usd_value / 20)
}
