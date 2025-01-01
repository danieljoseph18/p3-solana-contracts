import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { SolanaLiquidityPool } from "../target/types/solana_liquidity_pool";
import { PublicKey } from "@solana/web3.js";
import * as dotenv from "dotenv";

dotenv.config();

async function forceCloseUserState(targetUserPubkey: string) {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace
    .SolanaLiquidityPool as Program<SolanaLiquidityPool>;

  const [poolState] = PublicKey.findProgramAddressSync(
    [Buffer.from("pool-state")],
    program.programId
  );

  const targetUser = new PublicKey(targetUserPubkey);
  const [userState] = PublicKey.findProgramAddressSync(
    [Buffer.from("user-state"), targetUser.toBuffer()],
    program.programId
  );

  // Check if account exists before closing
  const accountBefore = await provider.connection.getAccountInfo(userState);
  console.log("Account exists before closing:", accountBefore !== null);
  if (accountBefore) {
    console.log("Account data size before:", accountBefore.data.length);
    console.log("Account lamports before:", accountBefore.lamports);
  }

  console.log("Force closing user state for:", targetUserPubkey);

  try {
    await program.methods
      .forceCloseUserState()
      .accountsStrict({
        admin: provider.wallet.publicKey,
        poolState: poolState,
        userState: userState,
        targetUser: targetUser,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // Verify account is closed
    const accountAfter = await provider.connection.getAccountInfo(userState);
    console.log("Account exists after closing:", accountAfter !== null);
    if (accountAfter) {
      console.log("Account data size after:", accountAfter.data.length);
      console.log("Account lamports after:", accountAfter.lamports);
    }

    console.log("User state force closed successfully!");
  } catch (error) {
    console.error("Failed to force close user state:", error);
    throw error;
  }
}

// Usage
forceCloseUserState("2WopEVinpz5MrjJcQppuvE2C5m14iPE5XNR8a2wsCs4C").catch(
  console.error
);
