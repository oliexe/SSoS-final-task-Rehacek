import * as anchor from "@project-serum/anchor";
import { Program, web3 } from "@project-serum/anchor";
import { PublicKey, SystemProgram } from "@solana/web3.js";
import { expect, assert } from "chai";
import { Auction } from "../target/types/auction";
import { getSOL, airdrop } from "./helpers";

describe("auction", async () => {
    const provider = anchor.AnchorProvider.local();
    anchor.setProvider(provider);
    const program = anchor.workspace.Auction as Program<Auction>;
    const state = anchor.web3.Keypair.generate();
    //Seller
    const seller = (program.provider as anchor.AnchorProvider).wallet;
    //Bidders
    const bidder2 = anchor.web3.Keypair.generate();
    const bidder1 = anchor.web3.Keypair.generate();
    //Treasury
    const [bidpoolPDA, _] = await PublicKey.findProgramAddress(
      [anchor.utils.bytes.utf8.encode("bid_pool_seed")],
      program.programId
    );
    //Settings
    const DURATION = 10;
    const BID_ONE = 2;
    const BID_TWO = 3;

    it("Fund bidders and seller", async () => {
      await airdrop(provider, seller.publicKey, getSOL(10));
      await airdrop(provider, bidder2.publicKey, getSOL(10));
      await airdrop(provider, bidder1.publicKey, getSOL(10));
    });

    it("Open up a new auction by seller", async () => {
      const tx = await program.methods
        .initialize(new anchor.BN(DURATION))
        .accounts({
          state: state.publicKey,
          bidPool: bidpoolPDA,
          seller: seller.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([state])
        .rpc();
      await provider.connection.confirmTransaction(tx);
      const currState = await program.account.state.fetch(state.publicKey);
      expect(currState.seller.toString()).to.eq(seller.publicKey.toString());
      expect(currState.winner.toString()).to.eq(PublicKey.default.toString());
      expect(currState.bidPool.toString()).to.eq(bidpoolPDA.toString());
      expect(currState.claimed).to.eq(false);
      const balance = await program.provider.connection.getBalance(bidpoolPDA);
      expect(balance).to.gt(0);
    });

    it("Execute bid (Seller 1 @ 2 SOL)", async () => {
      const bid = await createNewBid(bidder2, getSOL(BID_ONE));
      const bidState = await program.account.bidSlip.fetch(bid);
      expect(bidState.amount.toNumber()).to.eq(getSOL(BID_ONE));
      expect(bidState.fundsRefunded).to.eq(false);
      const balance = await program.provider.connection.getBalance(bidpoolPDA);
      expect(balance).to.gt(getSOL(BID_ONE));
    });

    it("Execute bid lower than the highest bid", async () => {
      try {
        await createNewBid(bidder1, getSOL(BID_ONE - 1));
        assert(false);
      } catch (e) {
        //Expects an error
        expect(e.error.errorCode.code).to.equal("LowBid");
      }
    });

    it("Execute bid (Seller 2 @ 3 SOL)", async () => {
      const bid = await createNewBid(bidder1, getSOL(BID_TWO));
      let currState = await program.account.state.fetch(state.publicKey);
      expect(currState.winner.toString()).to.eq(bidder1.publicKey.toString());
      expect(currState.topBid.toNumber()).to.eq(getSOL(BID_TWO));
      let bidState = await program.account.bidSlip.fetch(bid);
      expect(bidState.amount.toNumber()).to.eq(getSOL(BID_TWO));
    });

    it("Try to end auction after it's over", async () => {
      const before = await program.provider.connection.getBalance(
        bidpoolPDA
      );
      // wait until auction finished
      console.log("[ Waiting a few seconds for auction to close ]");
      await new Promise((resolve) => setTimeout(resolve, 10000));
      await endAuction(seller);
      const after = await program.provider.connection.getBalance(
        bidpoolPDA
      );
      expect(before - after).to.eq(getSOL(3));
      let currState = await program.account.state.fetch(state.publicKey);
      expect(currState.claimed).to.eq(true);
    });

    it("Losing bidder asking for refund (2SOL)", async () => {
      const bidPDA = await findBidSlip(bidder2);
      const before = await program.provider.connection.getBalance(bidpoolPDA);
      await refundAccount(bidder2);
      const after = await program.provider.connection.getBalance(bidpoolPDA);
      expect(before - after).to.eq(getSOL(2));
      let bidState = await program.account.bidSlip.fetch(bidPDA);
      expect(bidState.fundsRefunded).to.eq(true);
    });

    const findBidSlip = async (bidder: web3.Keypair): Promise<PublicKey> => {
      const [bidPDA, _] = await PublicKey.findProgramAddress(
        [ anchor.utils.bytes.utf8.encode("bid_slip_seed"),
          state.publicKey.toBytes(),
          bidder.publicKey.toBytes(),
        ],
        program.programId
      );

      return bidPDA;
    };

    const createNewBid = async (bidder: web3.Keypair, amount: number): Promise<PublicKey> => {
      const bidPDA = await findBidSlip(bidder);
      const tx = await program.methods
        .bid(new anchor.BN(amount))
        .accounts({
          bid: bidPDA,
          state: state.publicKey,
          bidPool: bidpoolPDA,
          bidder: bidder.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([bidder])
        .rpc();
      await provider.connection.confirmTransaction(tx);

      return bidPDA;
    };

    const endAuction = async seller => {
      const tx = await program.methods
        .endAuction()
        .accounts({
          state: state.publicKey,
          bidPool: bidpoolPDA,
          seller: seller.publicKey,
        })
        .rpc();
      await provider.connection.confirmTransaction(tx);
    };

    const refundAccount = async (bidder: web3.Keypair) => {
      const bidPDA = await findBidSlip(bidder);
      const tx = await program.methods
        .refund()
        .accounts({
          bid: bidPDA,
          state: state.publicKey,
          bidPool: bidpoolPDA,
          bidder: bidder.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([bidder2])
        .rpc();
      await provider.connection.confirmTransaction(tx);
    };
});
