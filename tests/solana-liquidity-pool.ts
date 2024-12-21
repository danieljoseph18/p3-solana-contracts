import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { SolanaLiquidityPool } from "../target/types/solana_liquidity_pool";
import {
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  createMint,
  createAccount,
  mintTo,
  getAccount,
  getAssociatedTokenAddress,
} from "@solana/spl-token";
import { PublicKey, LAMPORTS_PER_SOL, Keypair } from "@solana/web3.js";
import { assert } from "chai";
import BN from "bn.js";

describe("solana-liquidity-pool", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace
    .SolanaLiquidityPool as Program<SolanaLiquidityPool>;

  // Test accounts
  let admin: Keypair;
  let user1: Keypair;
  let user2: Keypair;

  // Program accounts
  let poolState: PublicKey;
  let poolBump: number;
  let solVault: PublicKey;
  let usdcVault: PublicKey;
  let lpTokenMint: PublicKey;
  let usdcRewardVault: PublicKey;

  // Token mints
  let solMint: PublicKey; // Wrapped SOL mint
  let usdcMint: PublicKey;

  // Mock Pyth accounts
  let pythSolPrice: Keypair;
  let pythUsdcPrice: Keypair;

  // User token accounts
  let user1SolAccount: PublicKey;
  let user1UsdcAccount: PublicKey;
  let user1LpTokenAccount: PublicKey;
  let user2SolAccount: PublicKey;
  let user2UsdcAccount: PublicKey;
  let user2LpTokenAccount: PublicKey;

  before(async () => {
    // Generate test accounts
    admin = Keypair.generate();
    user1 = Keypair.generate();
    user2 = Keypair.generate();

    // Airdrop SOL to admin and users
    await provider.connection.requestAirdrop(
      admin.publicKey,
      1000 * LAMPORTS_PER_SOL
    );
    await provider.connection.requestAirdrop(
      user1.publicKey,
      100 * LAMPORTS_PER_SOL
    );
    await provider.connection.requestAirdrop(
      user2.publicKey,
      100 * LAMPORTS_PER_SOL
    );

    // Create token mints
    solMint = await createMint(
      provider.connection,
      admin,
      admin.publicKey,
      null,
      9 // SOL has 9 decimals
    );

    usdcMint = await createMint(
      provider.connection,
      admin,
      admin.publicKey,
      null,
      6 // USDC has 6 decimals
    );

    // Create LP token mint
    lpTokenMint = await createMint(
      provider.connection,
      admin,
      admin.publicKey,
      null,
      9 // LP tokens use 9 decimals
    );

    // Create mock Pyth price accounts
    pythSolPrice = Keypair.generate();
    pythUsdcPrice = Keypair.generate();

    // Find PDA for pool state
    [poolState, poolBump] = await PublicKey.findProgramAddress(
      [Buffer.from("pool")],
      program.programId
    );

    // Get token vault addresses
    solVault = await getAssociatedTokenAddress(solMint, poolState, true);
    usdcVault = await getAssociatedTokenAddress(usdcMint, poolState, true);
    usdcRewardVault = await getAssociatedTokenAddress(
      usdcMint,
      poolState,
      true
    );

    // Create user token accounts
    user1SolAccount = await createAccount(
      provider.connection,
      user1,
      solMint,
      user1.publicKey
    );
    user1UsdcAccount = await createAccount(
      provider.connection,
      user1,
      usdcMint,
      user1.publicKey
    );
    user2SolAccount = await createAccount(
      provider.connection,
      user2,
      solMint,
      user2.publicKey
    );
    user2UsdcAccount = await createAccount(
      provider.connection,
      user2,
      usdcMint,
      user2.publicKey
    );

    // Create LP token accounts for users
    user1LpTokenAccount = await getAssociatedTokenAddress(
      lpTokenMint,
      user1.publicKey
    );
    user2LpTokenAccount = await getAssociatedTokenAddress(
      lpTokenMint,
      user2.publicKey
    );

    // Mint initial tokens to users
    await mintTo(
      provider.connection,
      admin,
      solMint,
      user1SolAccount,
      admin,
      100 * LAMPORTS_PER_SOL
    );
    await mintTo(
      provider.connection,
      admin,
      usdcMint,
      user1UsdcAccount,
      admin,
      10000_000000
    ); // $10,000 USDC
    await mintTo(
      provider.connection,
      admin,
      solMint,
      user2SolAccount,
      admin,
      50 * LAMPORTS_PER_SOL
    );
    await mintTo(
      provider.connection,
      admin,
      usdcMint,
      user2UsdcAccount,
      admin,
      5000_000000
    ); // $5,000 USDC
  });

  it("Initializes the pool", async () => {
    try {
      await program.methods
        .initialize()
        .accounts({
          admin: admin.publicKey,
          poolState,
          solVault,
          usdcVault,
          solMint,
          usdcMint,
          lpTokenMint,
          usdcRewardVault,
          systemProgram: anchor.web3.SystemProgram.programId,
          tokenProgram: TOKEN_PROGRAM_ID,
          rent: anchor.web3.SYSVAR_RENT_PUBKEY,
        })
        .signers([admin])
        .rpc();

      // Verify pool state
      const poolStateAccount = await program.account.poolState.fetch(poolState);
      assert.ok(poolStateAccount.admin.equals(admin.publicKey));
      assert.ok(poolStateAccount.solVault.equals(solVault));
      assert.ok(poolStateAccount.usdcVault.equals(usdcVault));
      assert.ok(poolStateAccount.lpTokenMint.equals(lpTokenMint));
      assert.equal(poolStateAccount.aumUsd.toNumber(), 0);
      assert.equal(poolStateAccount.tokensPerInterval.toNumber(), 0);
      assert.equal(poolStateAccount.rewardStartTime.toNumber(), 0);
      assert.equal(poolStateAccount.rewardEndTime.toNumber(), 0);
      assert.ok(poolStateAccount.usdcRewardVault.equals(usdcRewardVault));
      assert.equal(poolStateAccount.paused, false);
    } catch (err) {
      console.error("Error:", err);
      throw err;
    }
  });

  it("Deposits SOL into the pool", async () => {
    const depositAmount = new anchor.BN(10 * LAMPORTS_PER_SOL); // 10 SOL

    // Get user1's LP token account
    user1LpTokenAccount = await getAssociatedTokenAddress(
      lpTokenMint,
      user1.publicKey
    );

    const preBalances = {
      userSol: new BN(
        (
          await getAccount(provider.connection, user1SolAccount)
        ).amount.toString()
      ),
      poolSol: new BN(
        (await getAccount(provider.connection, solVault)).amount.toString()
      ),
      userLp: new BN(0),
    };

    await program.methods
      .deposit(depositAmount)
      .accounts({
        user: user1.publicKey,
        poolState,
        userState: await getUserStateAddress(user1.publicKey),
        tokenMint: solMint,
        solMint,
        tokenVault: solVault,
        userTokenAccount: user1SolAccount,
        lpTokenMint,
        userLpTokenAccount: user1LpTokenAccount,
        pythSolPrice: pythSolPrice.publicKey,
        pythUsdcPrice: pythUsdcPrice.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .signers([user1])
      .rpc();

    // Verify balances
    const postBalances = {
      userSol: new BN(
        (
          await getAccount(provider.connection, user1SolAccount)
        ).amount.toString()
      ),
      poolSol: new BN(
        (await getAccount(provider.connection, solVault)).amount.toString()
      ),
      userLp: new BN(
        (
          await getAccount(provider.connection, user1LpTokenAccount)
        ).amount.toString()
      ),
    };

    assert.equal(
      postBalances.userSol.toString(),
      preBalances.userSol.sub(depositAmount).toString()
    );
    assert.equal(
      postBalances.poolSol.toString(),
      preBalances.poolSol.add(depositAmount).toString()
    );
    assert.ok(postBalances.userLp.gt(new anchor.BN(0)));
  });

  it("Deposits USDC into the pool", async () => {
    const depositAmount = new anchor.BN(1000_000000); // $1,000 USDC

    // Get user2's LP token account
    user2LpTokenAccount = await getAssociatedTokenAddress(
      lpTokenMint,
      user2.publicKey
    );

    const preBalances = {
      userUsdc: new BN(
        (
          await getAccount(provider.connection, user2UsdcAccount)
        ).amount.toString()
      ),
      poolUsdc: new BN(
        (await getAccount(provider.connection, usdcVault)).amount.toString()
      ),
      userLp: new BN(0),
    };

    await program.methods
      .deposit(depositAmount)
      .accounts({
        user: user2.publicKey,
        poolState,
        userState: await getUserStateAddress(user2.publicKey),
        tokenMint: usdcMint,
        solMint,
        tokenVault: usdcVault,
        userTokenAccount: user2UsdcAccount,
        lpTokenMint,
        userLpTokenAccount: user2LpTokenAccount,
        pythSolPrice: pythSolPrice.publicKey,
        pythUsdcPrice: pythUsdcPrice.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .signers([user2])
      .rpc();

    // Verify balances
    const postBalances = {
      userUsdc: new BN(
        (
          await getAccount(provider.connection, user2UsdcAccount)
        ).amount.toString()
      ),
      poolUsdc: new BN(
        (await getAccount(provider.connection, usdcVault)).amount.toString()
      ),
      userLp: new BN(
        (
          await getAccount(provider.connection, user2LpTokenAccount)
        ).amount.toString()
      ),
    };

    assert.equal(
      postBalances.userUsdc.toString(),
      preBalances.userUsdc.sub(depositAmount).toString()
    );
    assert.equal(
      postBalances.poolUsdc.toString(),
      preBalances.poolUsdc.add(depositAmount).toString()
    );
    assert.ok(postBalances.userLp.gt(new anchor.BN(0)));
  });

  it("Withdraws SOL from the pool", async () => {
    // Get user1's current LP token balance
    const lpBalance = new BN(
      (
        await getAccount(provider.connection, user1LpTokenAccount)
      ).amount.toString()
    );
    const withdrawAmount = lpBalance.div(new anchor.BN(2)); // Withdraw half

    const preBalances = {
      userSol: new BN(
        (
          await getAccount(provider.connection, user1SolAccount)
        ).amount.toString()
      ),
      poolSol: new BN(
        (await getAccount(provider.connection, solVault)).amount.toString()
      ),
      userLp: lpBalance,
    };

    await program.methods
      .withdraw(withdrawAmount)
      .accounts({
        user: user1.publicKey,
        poolState,
        userState: await getUserStateAddress(user1.publicKey),
        tokenMint: solMint,
        solMint,
        tokenVault: solVault,
        userTokenAccount: user1SolAccount,
        lpTokenMint,
        userLpTokenAccount: user1LpTokenAccount,
        pythSolPrice: pythSolPrice.publicKey,
        pythUsdcPrice: pythUsdcPrice.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([user1])
      .rpc();

    // Verify balances
    const postBalances = {
      userSol: new BN(
        (
          await getAccount(provider.connection, user1SolAccount)
        ).amount.toString()
      ),
      poolSol: new BN(
        (await getAccount(provider.connection, solVault)).amount.toString()
      ),
      userLp: new BN(
        (
          await getAccount(provider.connection, user1LpTokenAccount)
        ).amount.toString()
      ),
    };

    assert.ok(postBalances.userSol.gt(preBalances.userSol));
    assert.ok(postBalances.poolSol.lt(preBalances.poolSol));
    assert.equal(
      postBalances.userLp.toString(),
      preBalances.userLp.sub(withdrawAmount).toString()
    );
  });

  it("Starts reward distribution", async () => {
    const rewardAmount = new anchor.BN(1000_000000); // 1,000 USDC
    const tokensPerInterval = new anchor.BN(1_000); // Adjust based on your needs

    // Mint reward tokens to admin
    const adminUsdcAccount = await createAccount(
      provider.connection,
      admin,
      usdcMint,
      admin.publicKey
    );
    await mintTo(
      provider.connection,
      admin,
      usdcMint,
      adminUsdcAccount,
      admin,
      rewardAmount.toNumber()
    );

    await program.methods
      .startRewards(rewardAmount, tokensPerInterval)
      .accounts({
        admin: admin.publicKey,
        poolState,
        adminUsdcAccount,
        usdcRewardVault,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([admin])
      .rpc();

    // Verify reward state
    const poolStateAccount = await program.account.poolState.fetch(poolState);
    assert.equal(
      poolStateAccount.tokensPerInterval.toString(),
      tokensPerInterval.toString()
    );
    assert.ok(poolStateAccount.rewardStartTime.gt(new anchor.BN(0)));
    assert.ok(
      poolStateAccount.rewardEndTime.gt(poolStateAccount.rewardStartTime)
    );
  });

  it("Claims rewards", async () => {
    // Wait a bit to accrue rewards
    await new Promise((resolve) => setTimeout(resolve, 5000));

    const preBalance = new BN(
      (
        await getAccount(provider.connection, user1UsdcAccount)
      ).amount.toString()
    );

    await program.methods
      .claimRewards()
      .accounts({
        user: user1.publicKey,
        poolState,
        userState: await getUserStateAddress(user1.publicKey),
        usdcRewardVault,
        userUsdcAccount: user1UsdcAccount,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([user1])
      .rpc();

    const postBalance = new BN(
      (
        await getAccount(provider.connection, user1UsdcAccount)
      ).amount.toString()
    );
    assert.ok(postBalance.gt(preBalance));
  });

  it("Admin can withdraw tokens", async () => {
    const withdrawAmount = new anchor.BN(1 * LAMPORTS_PER_SOL);
    const adminSolAccount = await createAccount(
      provider.connection,
      admin,
      solMint,
      admin.publicKey
    );

    const preBalance = new BN(
      (await getAccount(provider.connection, adminSolAccount)).amount.toString()
    );

    await program.methods
      .adminWithdraw(withdrawAmount)
      .accounts({
        admin: admin.publicKey,
        poolState,
        tokenMint: solMint,
        solMint,
        tokenVault: solVault,
        adminTokenAccount: adminSolAccount,
        tokenProgram: TOKEN_PROGRAM_ID,
        pythSolPrice: pythSolPrice.publicKey,
        pythUsdcPrice: pythUsdcPrice.publicKey,
      })
      .signers([admin])
      .rpc();

    const postBalance = new BN(
      (await getAccount(provider.connection, adminSolAccount)).amount.toString()
    );
    assert.equal(
      postBalance.toString(),
      preBalance.add(withdrawAmount).toString()
    );
  });

  it("Admin can pause and unpause the program", async () => {
    // Pause the program
    await program.methods
      .setPause(true)
      .accounts({
        admin: admin.publicKey,
        poolState,
      })
      .signers([admin])
      .rpc();

    let poolStateAccount = await program.account.poolState.fetch(poolState);
    assert.equal(poolStateAccount.paused, true);

    // Try to deposit while paused (should fail)
    try {
      await program.methods
        .deposit(new BN(1 * LAMPORTS_PER_SOL))
        .accounts({
          user: user1.publicKey,
          poolState,
          userState: await getUserStateAddress(user1.publicKey),
          tokenMint: solMint,
          solMint,
          tokenVault: solVault,
          userTokenAccount: user1SolAccount,
          lpTokenMint,
          userLpTokenAccount: user1LpTokenAccount,
          pythSolPrice: pythSolPrice.publicKey,
          pythUsdcPrice: pythUsdcPrice.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
          tokenProgram: TOKEN_PROGRAM_ID,
          associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
          rent: anchor.web3.SYSVAR_RENT_PUBKEY,
        })
        .signers([user1])
        .rpc();
      assert.fail("Should not be able to deposit while paused");
    } catch (error: any) {
      assert.include(error.toString(), "Program is paused");
    }

    // Unpause the program
    await program.methods
      .setPause(false)
      .accounts({
        admin: admin.publicKey,
        poolState,
      })
      .signers([admin])
      .rpc();

    poolStateAccount = await program.account.poolState.fetch(poolState);
    assert.equal(poolStateAccount.paused, false);
  });
});

// Helper function to derive user state address
async function getUserStateAddress(userPubkey: PublicKey): Promise<PublicKey> {
  const [userState] = await PublicKey.findProgramAddress(
    [Buffer.from("user_state"), userPubkey.toBuffer()],
    anchor.workspace.SolanaLiquidityPool.programId
  );
  return userState;
}
