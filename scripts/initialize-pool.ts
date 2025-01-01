import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { SolanaLiquidityPool } from "../target/types/solana_liquidity_pool";
import { PublicKey, Keypair, SystemProgram } from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  createMint,
  getOrCreateAssociatedTokenAccount,
  mintTo,
} from "@solana/spl-token";
import * as dotenv from "dotenv";

// Load environment variables
dotenv.config();

// Chainlink addresses (devnet & mainnet)
const CHAINLINK_PROGRAM_ID = new PublicKey(
  "HEvSKofvBgfaexv23kMabbYqxasxU3mQ4ibBMEmJWHny"
);

/**
 * On Devnet: 99B2bTijsU6f1GCT73HmdR7HCFFjGMBcPZY6jZ96ynrR
 * On Mainnet: CH31Xns5z3M1cTAbKW34jcxPPciazARpijcHj9rxtemt
 */
const CHAINLINK_SOL_FEED = process.env.IS_DEVNET
  ? new PublicKey("99B2bTijsU6f1GCT73HmdR7HCFFjGMBcPZY6jZ96ynrR")
  : new PublicKey("CH31Xns5z3M1cTAbKW34jcxPPciazARpijcHj9rxtemt");

/**
 * @dev to deploy run: anchor deploy --provider.cluster devnet
 */
async function main() {
  // Set up anchor provider
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace
    .SolanaLiquidityPool as Program<SolanaLiquidityPool>;

  console.log("Program ID:", program.programId.toString());

  // Create SOL and mock USDC mints
  console.log("Creating token mints...");
  const solMint = new PublicKey("So11111111111111111111111111111111111111112");
  const usdcMint = process.env.IS_DEVNET
    ? new PublicKey("7ggkvgP7jijLpQBV5GXcqugTMrc2JqDi9tiCH36SVg7A")
    : new PublicKey("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");

  // Find pool state PDA
  const [poolState, poolStateBump] = PublicKey.findProgramAddressSync(
    [Buffer.from("pool-state")],
    program.programId
  );
  console.log(
    "Pool state PDA:",
    poolState.toString(),
    "with bump:",
    poolStateBump,
    "using seed:",
    "pool-state"
  );

  // Create vault accounts
  console.log("Creating vault accounts...");
  let solVault = await getOrCreateAssociatedTokenAccount(
    provider.connection,
    (provider.wallet as anchor.Wallet).payer,
    solMint,
    poolState, // Set Pool State as vault owner.
    true
  );

  console.log("SOL vault created:", solVault.address.toString());

  let usdcVault = await getOrCreateAssociatedTokenAccount(
    provider.connection,
    (provider.wallet as anchor.Wallet).payer,
    usdcMint, // Use mock USDC mint
    poolState,
    true
  );

  console.log("USDC vault created:", usdcVault.address.toString());

  // Create LP token mint
  console.log("Creating LP token mint...");
  const lpTokenMintKeypair = Keypair.generate();
  await createMint(
    provider.connection,
    (provider.wallet as anchor.Wallet).payer,
    poolState, // mint authority
    poolState, // freeze authority
    6, // decimals
    lpTokenMintKeypair
  );
  console.log(
    "LP token mint created:",
    lpTokenMintKeypair.publicKey.toString()
  );

  // Initialize the pool
  console.log("Initializing pool...");
  try {
    await program.methods
      .initialize()
      .accountsStrict({
        admin: provider.wallet.publicKey,
        poolState,
        solVault: solVault.address,
        usdcVault: usdcVault.address,
        lpTokenMint: lpTokenMintKeypair.publicKey,
        usdcRewardVault: usdcVault.address,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .signers([lpTokenMintKeypair])
      .rpc();

    console.log("Pool initialized successfully!");

    // Log important addresses for future reference
    console.log("\nImportant addresses:");
    console.log("USDC Mint:", usdcMint.toString());
    console.log("Pool State:", poolState.toString());
    console.log("SOL Vault:", solVault.address.toString());
    console.log("USDC Vault:", usdcVault.address.toString());
    console.log("LP Token Mint:", lpTokenMintKeypair.publicKey.toString());
  } catch (error) {
    console.error("Failed to initialize pool:", error);
    throw error;
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
