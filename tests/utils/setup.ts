import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { SolanaLiquidityPool } from "../../target/types/solana_liquidity_pool";
import { PublicKey, Keypair, Connection } from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  createMint,
  createAccount,
  mintTo,
  getOrCreateAssociatedTokenAccount,
} from "@solana/spl-token";

// Chainlink program and feed addresses
export const CHAINLINK_PROGRAM_ID = new PublicKey(
  "HEvSKofvBgfaexv23kMabbYqxasxU3mQ4ibBMEmJWHny"
);
export const CHAINLINK_SOL_FEED = new PublicKey(
  "FmAmfoyPXiA8Vhhe6MZTr3U6rZfEZ1ctEHay1ysqCqcf"
);

export const createTestContext = async () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace
    .SolanaLiquidityPool as Program<SolanaLiquidityPool>;

  return {
    provider,
    program,
  };
};

export const createTestToken = async (
  provider: anchor.AnchorProvider,
  decimals: number = 6
): Promise<PublicKey> => {
  const mint = await createMint(
    provider.connection,
    (provider.wallet as anchor.Wallet).payer,
    provider.wallet.publicKey,
    null,
    decimals
  );

  return mint;
};

export const createTokenAccount = async (
  provider: anchor.AnchorProvider,
  mint: PublicKey,
  owner: PublicKey
): Promise<PublicKey> => {
  const account = await getOrCreateAssociatedTokenAccount(
    provider.connection,
    (provider.wallet as anchor.Wallet).payer,
    mint,
    owner
  );

  return account.address;
};

export const airdropSol = async (
  provider: anchor.AnchorProvider,
  target: PublicKey,
  amount: number
): Promise<void> => {
  const signature = await provider.connection.requestAirdrop(
    target,
    amount * anchor.web3.LAMPORTS_PER_SOL
  );
  const latestBlockhash = await provider.connection.getLatestBlockhash();
  await provider.connection.confirmTransaction({
    signature,
    ...latestBlockhash,
  });
};
