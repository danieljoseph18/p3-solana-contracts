import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { SolanaLiquidityPool } from "../target/types/solana_liquidity_pool";
import { PublicKey } from "@solana/web3.js";
import * as dotenv from "dotenv";

// Load environment variables
dotenv.config();

async function main() {
  // Configure the client
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace
    .SolanaLiquidityPool as Program<SolanaLiquidityPool>;

  console.log("Program ID:", program.programId.toString());

  // Derive the pool state PDA
  const [poolState] = PublicKey.findProgramAddressSync(
    [Buffer.from("pool-state")],
    program.programId
  );

  console.log("Closing pool state:", poolState.toString());

  try {
    await program.methods
      .closePool()
      .accountsStrict({
        admin: provider.wallet.publicKey,
        poolState: poolState,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    console.log("Pool closed successfully!");
  } catch (error) {
    console.error("Failed to close pool:", error);
    throw error;
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
