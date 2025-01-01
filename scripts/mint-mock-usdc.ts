import * as anchor from "@coral-xyz/anchor";
import { PublicKey } from "@solana/web3.js";
import { getOrCreateAssociatedTokenAccount, mintTo } from "@solana/spl-token";
import * as dotenv from "dotenv";

dotenv.config();

async function main() {
  // Set up anchor provider
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  // Get the mock USDC mint address from command line argument
  const mockUsdcMint = new PublicKey(process.argv[2]);
  const amount = parseInt(process.argv[3]);

  if (!mockUsdcMint || !amount) {
    console.error(
      "Please provide the mock USDC mint address and amount to mint as command line arguments"
    );
    console.error(
      "Usage: ts-node scripts/mint-mock-usdc.ts <mock-usdc-mint-address> <amount>"
    );
    process.exit(1);
  }

  console.log("Mock USDC mint:", mockUsdcMint.toString());
  console.log(`Amount to mint: ${amount} USDC`);

  try {
    // Get or create the recipient's token account
    const recipientTokenAccount = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      (provider.wallet as anchor.Wallet).payer,
      mockUsdcMint,
      provider.wallet.publicKey
    );

    // Mint the tokens (remember to multiply by 10^6 for USDC's 6 decimals)
    await mintTo(
      provider.connection,
      (provider.wallet as anchor.Wallet).payer,
      mockUsdcMint,
      recipientTokenAccount.address,
      provider.wallet.publicKey,
      amount * 1_000000 // Convert to 6 decimal places
    );

    console.log(
      `Successfully minted ${amount} mock USDC to ${provider.wallet.publicKey}`
    );
    console.log(
      "Recipient token account:",
      recipientTokenAccount.address.toString()
    );
  } catch (error) {
    console.error("Failed to mint tokens:", error);
    throw error;
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
