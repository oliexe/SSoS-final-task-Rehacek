use anchor_lang::prelude::*;
use anchor_lang::solana_program::{program::invoke, system_instruction};

declare_id!("HkTuJtyhu7yFYGkZ5cyZrvYS6By6rPxRo4ydfnbYPQ7T");

#[program]
pub mod auction {
    use super::*;

    /// Creates and initialize a new state of our program
    pub fn initialize(ctx: Context<Initialize>, end_at: i64) -> Result<()> {
        msg!(":: Initializing Auction ::");
        //Initialize the auction account, setting up the seller, using clock sysvar calculate the end time
        let state: &mut Account<State> = &mut ctx.accounts.state;
        let end_at_calc = Clock::get()?.unix_timestamp.checked_add(end_at);

        state.claimed = false;
        state.seller = *ctx.accounts.seller.key;
        state.end_at = end_at_calc.unwrap();
        state.winner = Pubkey::default();

        //Setup the bid pool account
        let bid_pool = &mut ctx.accounts.bid_pool;
        bid_pool.bump = *ctx.bumps.get("bid_pool").unwrap();
        state.bid_pool = ctx.accounts.bid_pool.key();

        Ok(())
    }

    /// BidSlip
    pub fn bid(ctx: Context<BidAuction>, bid_amount: u64) -> Result<()> {
        msg!(":: Placing bid on Auction ::");
        let state: &mut Account<State> = &mut ctx.accounts.state;
        let bidder_account = &mut ctx.accounts.bidder;
        let treasury_account = &mut ctx.accounts.bid_pool;

        //Check if the bid is higher than the auction's top bid
        if bid_amount < state.top_bid {
            return Err(error!(AuctionErrors::LowBid));
        }

        //Check if the specific auction is still open
        if Clock::get()?.unix_timestamp >= state.end_at {
            return Err(error!(AuctionErrors::AuctionClosed));
        }

        //Transfer funds via system_instruction::transfer (Transfering funds from bidder account to bid_pool)
        invoke(
            &system_instruction::transfer(bidder_account.key, &treasury_account.key(), bid_amount),
            &[
                bidder_account.to_account_info().clone(),
                treasury_account.to_account_info().clone(),
            ],
        )?;

        //Bid information is stored in the associated BidNote account
        let bid_account = &mut ctx.accounts.bid;
        bid_account.amount = bid_amount;
        bid_account.bump = *ctx.bumps.get("bid").unwrap();
        bid_account.funds_refunded = false;
        state.top_bid = bid_amount;
        state.winner = *bidder_account.key;

        Ok(())
    }

    /// After an auction ends (determined by `end_at`), a seller can claim the
    /// heighest bid by calling this instruction
    pub fn end_auction(ctx: Context<EndAuction>) -> Result<()> {
        msg!(":: Closing Auction ::");
        let state: &mut Account<State> = &mut ctx.accounts.state;
        let seller = &mut ctx.accounts.seller;

        //Ensure the auction can be claimed only after it ended
        if Clock::get()?.unix_timestamp < state.end_at {
            return Err(error!(AuctionErrors::AuctionNotFinished));
        }

        //Check if it's the seller who is ending the auction
        if state.seller != seller.key() {
            return Err(error!(AuctionErrors::Unauthorized));
        }

        //Check if the auction is not already claimed
        if state.claimed {
            return Err(error!(AuctionErrors::AlreadyClaimed));
        }

        state.claimed = true;
        **ctx.accounts.bid_pool.to_account_info().try_borrow_mut_lamports()? -= state.top_bid;
        **ctx.accounts.seller.try_borrow_mut_lamports()? += state.top_bid;

        Ok(())
    }

    /// After an auction ends (the initializer/seller already received the winning bid),
    /// the unsuccessfull bidders can claim their money back by calling this instruction
    pub fn refund(ctx: Context<Refund>) -> Result<()> {
        msg!(":: Refunding from bid_pool ::");
        let state: &mut Account<State> = &mut ctx.accounts.state;
        let bid: &mut Account<BidSlip> = &mut ctx.accounts.bid;
        let bidder = &mut ctx.accounts.bidder;

        //Ensure the refund can be claimed only after auction is finished
        if Clock::get()?.unix_timestamp < state.end_at {
            return Err(error!(AuctionErrors::AuctionNotFinished));
        }

        //Winner of the auction should not be able to refund
        if state.winner == bidder.key() {
            return Err(error!(AuctionErrors::Unauthorized));
        }

        //Ensure only one refund can be made
        if bid.funds_refunded {
            return Err(error!(AuctionErrors::AlreadyRefunded));
        }

        bid.funds_refunded = true;
        **ctx.accounts.bid_pool.to_account_info().try_borrow_mut_lamports()? -= bid.amount;
        **ctx.accounts.bidder.try_borrow_mut_lamports()? += bid.amount;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init,payer = seller, space = 300)]
    pub state: Account<'info, State>,
    #[account(init,payer = seller,space = 10,seeds = [b"bid_pool_seed"], bump)]
    pub bid_pool: Account<'info, BidPool>,
    #[account(mut)]
    pub seller: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct BidAuction<'info> {
    #[account(init,payer = bidder,space = 152,seeds = [b"bid_slip_seed", state.key().as_ref(), bidder.key().as_ref()],bump,)]
    pub bid: Account<'info, BidSlip>,
    #[account(mut, has_one = bid_pool)]
    pub state: Account<'info, State>,
    #[account(mut, seeds = [b"bid_pool_seed"], bump = bid_pool.bump)]
    pub bid_pool: Account<'info, BidPool>,
    #[account(mut)]
    pub bidder: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Refund<'info> {
    #[account(mut, seeds = [b"bid_slip_seed", state.key().as_ref(), bidder.key().as_ref()], bump = bid.bump)]
    pub bid: Account<'info, BidSlip>,
    #[account(has_one = bid_pool)]
    pub state: Account<'info, State>,
    #[account(mut, seeds = [b"bid_pool_seed"], bump = bid_pool.bump)]
    pub bid_pool: Account<'info, BidPool>,
    #[account(mut)]
    pub bidder: Signer<'info>,
}

#[derive(Accounts)]
pub struct EndAuction<'info> {
    #[account(mut, has_one = bid_pool)]
    pub state: Account<'info, State>,
    #[account(mut, seeds = [b"bid_pool_seed"], bump = bid_pool.bump)]
    pub bid_pool: Account<'info, BidPool>,
    #[account(mut)]
    pub seller: Signer<'info>,
}

#[account]
pub struct State {
    pub seller: Pubkey,
    pub end_at: i64,
    pub top_bid: u64,
    pub winner: Pubkey,
    pub bid_pool: Pubkey,
    pub claimed: bool,
}

//PDAs
#[account]
#[derive(Default)]
pub struct BidPool {
    pub bump: u8,
}

#[account]
#[derive(Default)]
pub struct BidSlip {
    pub amount: u64,
    pub bid_at: i64,
    pub bump: u8,
    pub funds_refunded: bool,
}

#[error_code]
pub enum AuctionErrors {
    #[msg("AuctionNotFinished")]
    AuctionNotFinished,
    #[msg("LowBid")]
    LowBid,
    #[msg("NotEnoughFunds")]
    NotEnoughFunds,
    #[msg("AuctionClosed")]
    AuctionClosed,
    #[msg("AlreadyClaimed")]
    AlreadyClaimed,
    #[msg("Unauthorized")]
    Unauthorized,
    #[msg("AlreadyRefunded")]
    AlreadyRefunded,
}
