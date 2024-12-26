# Solana Liquidity Pool Program

A Solana program for managing a liquidity pool with staking and rewards functionality.

## Program Address

- Devnet: `CkpZTxULEPgWHKkmWcNdvBR4SkijmUMY3sRYurGeTTvF`

## Prerequisites

```bash
npm install @coral-xyz/anchor @solana/web3.js @solana/spl-token
```

## Frontend Integration Guide

### 1. Initialize Connection and Program

```typescript
import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Connection, PublicKey, Keypair } from "@solana/web3.js";
import { SolanaLiquidityPool } from "./types/solana_liquidity_pool"; // Generated types from your IDL

// Initialize connection
const connection = new Connection("https://api.devnet.solana.com");

// Initialize provider
const provider = new anchor.AnchorProvider(
  connection,
  window.solana, // or your wallet adapter
  { commitment: "confirmed" }
);

// Initialize program
const program = new Program<SolanaLiquidityPool>(
  IDL,
  "CkpZTxULEPgWHKkmWcNdvBR4SkijmUMY3sRYurGeTTvF",
  provider
);
```

### 2. Program Instructions

#### Initialize Pool

```typescript
const initializePool = async () => {
  // Generate a new keypair for the pool
  const poolKeypair = Keypair.generate();

  try {
    const tx = await program.methods
      .initialize()
      .accounts({
        pool: poolKeypair.publicKey,
        authority: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([poolKeypair])
      .rpc();

    console.log("Pool initialized:", tx);
    return poolKeypair.publicKey;
  } catch (error) {
    console.error("Error initializing pool:", error);
    throw error;
  }
};
```

#### Deposit

```typescript
const deposit = async (poolAddress: PublicKey, amount: number) => {
  try {
    const tx = await program.methods
      .deposit(new anchor.BN(amount))
      .accounts({
        pool: poolAddress,
        user: provider.wallet.publicKey,
        userTokenAccount: userTokenAccount, // Your token account
        poolTokenAccount: poolTokenAccount, // Pool's token account
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();

    console.log("Deposit successful:", tx);
  } catch (error) {
    console.error("Error depositing:", error);
    throw error;
  }
};
```

#### Withdraw

```typescript
const withdraw = async (poolAddress: PublicKey, amount: number) => {
  try {
    const tx = await program.methods
      .withdraw(new anchor.BN(amount))
      .accounts({
        pool: poolAddress,
        user: provider.wallet.publicKey,
        userTokenAccount: userTokenAccount,
        poolTokenAccount: poolTokenAccount,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();

    console.log("Withdrawal successful:", tx);
  } catch (error) {
    console.error("Error withdrawing:", error);
    throw error;
  }
};
```

#### Start Rewards

```typescript
const startRewards = async (
  poolAddress: PublicKey,
  rewardRate: number,
  duration: number
) => {
  try {
    const tx = await program.methods
      .startRewards(new anchor.BN(rewardRate), new anchor.BN(duration))
      .accounts({
        pool: poolAddress,
        authority: provider.wallet.publicKey,
      })
      .rpc();

    console.log("Rewards started:", tx);
  } catch (error) {
    console.error("Error starting rewards:", error);
    throw error;
  }
};
```

#### Claim Rewards

```typescript
const claimRewards = async (poolAddress: PublicKey) => {
  try {
    const tx = await program.methods
      .claimRewards()
      .accounts({
        pool: poolAddress,
        user: provider.wallet.publicKey,
        userTokenAccount: userRewardTokenAccount,
        poolTokenAccount: poolRewardTokenAccount,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();

    console.log("Rewards claimed:", tx);
  } catch (error) {
    console.error("Error claiming rewards:", error);
    throw error;
  }
};
```

### 3. Fetching Pool Data

```typescript
const getPoolData = async (poolAddress: PublicKey) => {
  try {
    const poolAccount = await program.account.pool.fetch(poolAddress);
    return {
      authority: poolAccount.authority,
      totalStaked: poolAccount.totalStaked.toString(),
      rewardRate: poolAccount.rewardRate.toString(),
      lastUpdateTime: poolAccount.lastUpdateTime.toString(),
      rewardDuration: poolAccount.rewardDuration.toString(),
      // ... other pool data
    };
  } catch (error) {
    console.error("Error fetching pool data:", error);
    throw error;
  }
};
```

### 4. Fetching User Data

```typescript
const getUserStakeData = async (
  poolAddress: PublicKey,
  userAddress: PublicKey
) => {
  try {
    const [userStakeAccount] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("user_stake"),
        poolAddress.toBuffer(),
        userAddress.toBuffer(),
      ],
      program.programId
    );

    const userStakeData = await program.account.userStake.fetch(
      userStakeAccount
    );
    return {
      amount: userStakeData.amount.toString(),
      rewardDebt: userStakeData.rewardDebt.toString(),
      // ... other user data
    };
  } catch (error) {
    console.error("Error fetching user stake data:", error);
    throw error;
  }
};
```

## Error Handling

The program defines custom errors that you should handle in your frontend:

```typescript
try {
  // ... program instruction
} catch (error) {
  if (error.code === 6000) {
    console.error("Insufficient balance");
  } else if (error.code === 6001) {
    console.error("Invalid amount");
  }
  // ... handle other custom errors
}
```

## Event Listening

You can listen to program events using the connection's onProgramAccountChange:

```typescript
const subscribeToPoolChanges = (poolAddress: PublicKey) => {
  const subscriptionId = connection.onAccountChange(
    poolAddress,
    (accountInfo) => {
      const decodedData = program.coder.accounts.decode(
        "pool",
        accountInfo.data
      );
      console.log("Pool updated:", decodedData);
    }
  );

  return subscriptionId; // Save this to unsubscribe later
};
```

## Testing

For testing your frontend integration, you can use the Solana devnet:

1. Switch to devnet in your Phantom wallet or other wallet adapter
2. Get devnet SOL from the [Solana Faucet](https://solfaucet.com/)
3. Create test tokens using the SPL Token program

## Resources

- [Solana Explorer (Devnet)](https://explorer.solana.com/?cluster=devnet)
- [Anchor Documentation](https://www.anchor-lang.com/)
- [Solana Web3.js Documentation](https://solana-labs.github.io/solana-web3.js/)
- [SPL Token Documentation](https://spl.solana.com/token)
