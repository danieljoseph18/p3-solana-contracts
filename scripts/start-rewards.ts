import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { SolanaLiquidityPool } from "../target/types/solana_liquidity_pool";
import { PublicKey } from "@solana/web3.js";
import {
  getOrCreateAssociatedTokenAccount,
  getAccount,
} from "@solana/spl-token";
import * as dotenv from "dotenv";

dotenv.config();

async function main() {
  // Configure the client
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace
    .SolanaLiquidityPool as Program<SolanaLiquidityPool>;

  console.log("Program ID:", program.programId.toString());

  // Get command line arguments for USDC amount
  const usdcAmount = parseInt(process.argv[2]);

  if (!usdcAmount) {
    console.error("Please provide USDC amount as command line argument");
    console.error("Usage: ts-node scripts/start-rewards.ts <usdc-amount>");
    process.exit(1);
  }

  // Convert USDC amount to proper decimals (6)
  const usdcAmountWithDecimals = usdcAmount * 1_000000;

  // Derive the pool state PDA
  const [poolState] = PublicKey.findProgramAddressSync(
    [Buffer.from("pool-state")],
    program.programId
  );

  // Fetch pool state to get USDC reward vault
  const poolStateAccount = await program.account.poolState.fetch(poolState);
  const usdcRewardVault = poolStateAccount.usdcRewardVault;

  // Fetch the token account data to get the mint
  const rewardVaultAccount = await getAccount(
    provider.connection,
    usdcRewardVault
  );

  // Get admin's USDC token account
  const adminUsdcAccount = await getOrCreateAssociatedTokenAccount(
    provider.connection,
    (provider.wallet as anchor.Wallet).payer,
    rewardVaultAccount.mint,
    provider.wallet.publicKey
  );

  console.log("Starting rewards distribution...");
  console.log(`USDC Amount: ${usdcAmount} USDC`);

  try {
    await program.methods
      .startRewards(new anchor.BN(usdcAmountWithDecimals), new anchor.BN(0)) // tokens_per_interval is calculated on-chain
      .accountsStrict({
        admin: provider.wallet.publicKey,
        poolState,
        adminUsdcAccount: adminUsdcAccount.address,
        usdcRewardVault,
        tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
      })
      .rpc();

    console.log("Successfully started rewards distribution!");
    console.log(`Deposited: ${usdcAmount} USDC`);
    console.log("Duration: 7 days (604800 seconds)");
    console.log(`Rate: ${usdcAmount / 604800} USDC per second`);
  } catch (error) {
    console.error("Failed to start rewards:", error);
    throw error;
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
