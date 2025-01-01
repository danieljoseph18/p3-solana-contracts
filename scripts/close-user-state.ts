import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { SolanaLiquidityPool } from "../target/types/solana_liquidity_pool";
import { PublicKey } from "@solana/web3.js";
import * as dotenv from "dotenv";

// Load environment variables
dotenv.config();

async function closeUserState(targetUserPubkey: string) {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace
    .SolanaLiquidityPool as Program<SolanaLiquidityPool>;

  // Find PDAs
  const [poolState] = PublicKey.findProgramAddressSync(
    [Buffer.from("pool-state")],
    program.programId
  );

  const [userState] = PublicKey.findProgramAddressSync(
    [Buffer.from("user-state"), new PublicKey(targetUserPubkey).toBuffer()],
    program.programId
  );

  console.log("Closing user state for:", targetUserPubkey);

  try {
    await program.methods
      .closeUserState()
      .accountsStrict({
        user: provider.wallet.publicKey, // This is you (the admin)
        poolState: poolState,
        userState: userState,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    console.log("User state closed successfully!");
  } catch (error) {
    console.error("Failed to close user state:", error);
    throw error;
  }
}

// Usage:
closeUserState("2WopEVinpz5MrjJcQppuvE2C5m14iPE5XNR8a2wsCs4C").catch(
  console.error
);
