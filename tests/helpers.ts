import { LAMPORTS_PER_SOL } from "@solana/web3.js";

export function getSOL(sol: number) {
  return sol * LAMPORTS_PER_SOL;
}

export async function airdrop(provider, publicKey, amount: number) {
  await provider.connection.confirmTransaction(
    await provider.connection.requestAirdrop(publicKey, amount),
    "confirmed"
  );
}
