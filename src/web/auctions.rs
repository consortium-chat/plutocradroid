use super::prelude::*;
use crate::models::{AuctionWinner, Transfer, TransferExtra};

#[derive(Debug, Clone, FromForm)]
pub struct BidForm {
    csrf: String,
    amount: i64,
    is_max_bid: bool,
}

fn display_auction(auction:&AuctionWinner) -> maud::Markup {
    maud::html!{
        div class=(if auction.finished { "auction auction-finished" } else { "auction auction-pending" }) {
            div style="font-weight: bold" {
                a href=(format!("/auctions/{}", auction.damm())) {
                    "Auction#"
                    (auction.damm())
                }
            }
            div {
                (auction.auctioneer_name())
                @if auction.finished {
                    " offered "
                } @else {
                    " offers "
                }
                (auction.offer_amt) " " (auction.offer_ty)
                " for "
                (auction.bid_ty)
                "."
                br;
                @if auction.finished {
                    @if let Some(winner_id) = auction.winner_id {
                        "Auction won by "
                        (crate::names::name_of(winner_id.into_serenity()))
                        " for "
                        (auction.winner_bid.unwrap()) " " (auction.bid_ty)
                        "."
                    } @else {
                        "Auction expired with no winner."
                    }
                } @else {
                    @if let Some(winner_id) = auction.winner_id {
                        "Current bid is "
                        (auction.winner_bid.unwrap()) " " (auction.bid_ty)
                        " by "
                        (crate::names::name_of(winner_id.into_serenity()))
                    } @else {
                        "No bids. Minimum bid is " (auction.bid_min) " " (auction.bid_ty) "."
                    }
                    br;
                    "Auction will end at "
                    (super::template::show_ts(auction.end_at()))
                    " if no further bids are placed."
                }
            }
        }
    }
}


#[post("/auctions/<damm_id>/bid", data = "<data>")]
pub fn auction_bid(
    mut ctx: CommonContext,
    data: LenientForm<BidForm>,
    damm_id: String,
) -> PlutoResponse {
    let now = Utc::now();
    let id:i64 = if let Some(digits) = crate::damm::validate_ascii(damm_id.as_str()) {
        atoi::atoi(digits.as_slice()).unwrap()
    } else {
        info!("bad id");
        return hard_err(rocket::http::Status::NotFound);
    };
    if ctx.cookies.get(CSRF_COOKIE_NAME).map(|token| token.value()) != Some(data.csrf.as_str()) {
        return hard_err(rocket::http::Status::BadRequest);
    }

    let deets:&Deets = if let Some(d) = ctx.deets.as_ref() {
        d
    } else {
        info!("no deets");
        return hard_err(rocket::http::Status::Unauthorized);
    };

    if data.amount < 0 {
        return hard_err(rocket::http::Status::BadRequest);
    }

    let mut res:Option<PlutoResponse> = None;
    let mut status_msg:Option<String> = None;

    let transaction_res = ctx.conn.transaction::<_,diesel::result::Error,_>(|| {
        use schema::auctions::dsl as adsl;
        use view_schema::auction_and_winner::dsl as anw;
        
        // This needs to be a separate statement from the AuctionWinner because AuctionWinner joins multiple tables
        // and thus cannot be locked with .for_update().
        let maybe_auction_id:Option<i64> = adsl::auctions
            .select(adsl::rowid)
            .filter(adsl::rowid.eq(id))
            .for_update()
            .get_result(&*ctx)
            .optional()
            .unwrap();

        if maybe_auction_id.is_none() {
            res = Some(hard_err(rocket::http::Status::NotFound));
            return Ok(());
        }

        let auction:AuctionWinner = anw::auction_and_winner
            .select(AuctionWinner::cols())
            .filter(anw::auction_id.eq(id))
            .get_result(&*ctx)
            .unwrap();

        if now > auction.end_at() {
            status_msg = Some("Bid failed: Auction has ended".to_string());
            return Ok(());
        }

        if data.amount < auction.current_min_bid() {
            status_msg = Some("Bid failed: You must bid more than that.".to_string());
            return Ok(());
        }

        let mut to_lock = vec![deets.id()];
        if let Some(prev_bidder) = auction.winner_id {
            if prev_bidder != deets.id() {
                to_lock.push(prev_bidder);
            }
        }

        let mut handle = TransferHandler::new(
            &*ctx,
            to_lock,
            vec![auction.offer_ty.clone(), auction.bid_ty.clone()],
        )?;

        let challenger_id = deets.id();

        let mut challenger_available_balance = handle.balance(challenger_id, auction.bid_ty.clone());
        if let Some((user, amt)) = auction.winner() {
            if user == challenger_id {
                challenger_available_balance = challenger_available_balance.checked_add(amt).unwrap();
            }
        }
        
        // This test must be *before* the bid is tested against the current max bid; Otherwise the bid could be increased if someone else has a higher max bid, regardless of if you have the dough to support it.
        if challenger_available_balance < data.amount {
            status_msg = Some(
                format!(
                    "Bid failed: You do not have enough {}",
                    auction.bid_ty
                )
            );
            return Ok(());
        }

        let maybe_old_bid = auction.winner();

        let new_bid;
        let new_max_bid;

        if let Some(max_bid_bad) = auction.max_bid() {
            let max_bid_user = max_bid_bad.user;
            
            let attempted_max_bid = max_bid_bad.amount;
            // The highest value that could be the actual max bid
            let max_max_bid = handle.balance(max_bid_bad.user, max_bid_bad.currency) + auction.winner().unwrap().1;
            let max_bid_amount = if attempted_max_bid > max_max_bid { max_max_bid } else { attempted_max_bid };

            if challenger_id == max_bid_user {
                if data.is_max_bid {
                    new_bid = (max_bid_user, auction.winner().unwrap().1);
                    new_max_bid = Some((max_bid_user, data.amount));
                    status_msg = Some(
                        format!(
                            "You have set your max bid to {amount}{ty}.",
                            ty = auction.bid_ty,
                            amount = data.amount,
                        )
                    );
                } else if data.amount >= auction.winner().unwrap().1 {
                    new_bid = (max_bid_user, data.amount);
                    new_max_bid = Some((max_bid_user, attempted_max_bid));
                    status_msg = Some(
                        format!(
                            "You have increased your actual bid to {amount}{ty}",
                            ty = auction.bid_ty,
                            amount = data.amount,
                        )
                    );
                } else {
                    unreachable!();
                }
            } else if data.amount <= max_bid_amount {
                new_bid = (max_bid_user, data.amount);
                new_max_bid = Some((max_bid_user, attempted_max_bid));
                status_msg = Some(
                    format!(
                        "Your bid was not greater than {champion}'s existing max bid; The bid is now at {amount}{ty}.",
                        ty = auction.bid_ty,
                        champion = name_of(new_bid.0.into_serenity()),
                        amount = new_bid.1,
                    )
                );
            } else if data.is_max_bid {
                new_bid = (challenger_id, max_bid_amount.checked_add(1).unwrap());
                new_max_bid = Some((challenger_id, data.amount));
                status_msg = Some(
                    format!(
                        "You have set a max bid of {max}{ty}. The bid is now at {amount}{ty}",
                        ty = auction.bid_ty,
                        max = data.amount,
                        amount = new_bid.1,
                    )
                );
            } else {
                new_bid = (challenger_id, data.amount);
                new_max_bid = None;
                status_msg = Some(
                    format!(
                        "You have successfully bid {amount}{ty}",
                        ty = auction.bid_ty,
                        amount = new_bid.1
                    )
                );
            }
        } else if let Some(old_bid) = auction.winner() {
            if data.amount <= old_bid.1 {
                new_bid = old_bid;
                new_max_bid = None;
                status_msg = Some(
                    format!(
                        "Your bid is below the current bid of {amount}{ty}.",
                        ty = auction.bid_ty,
                        amount = old_bid.1,
                    )
                );
            } else if data.is_max_bid {
                if challenger_id == old_bid.0 {
                    new_bid = old_bid;
                } else {
                    new_bid = (challenger_id, old_bid.1.checked_add(1).unwrap());
                }
                new_max_bid = Some((challenger_id, data.amount));
                status_msg = Some(
                    format!(
                        "You have set a max bid of {max}{ty}. The bid is now {amount}{ty}",
                        ty = auction.bid_ty,
                        max = data.amount,
                        amount = new_bid.1,
                    )
                );
            } else {
                new_bid = (challenger_id, data.amount);
                new_max_bid = None;
                status_msg = Some(
                    format!(
                        "You have successfully bid {amount}{ty}",
                        ty = auction.bid_ty,
                        amount = new_bid.1,
                    )
                )
            }
        } else if data.amount >= auction.bid_min {
            if data.is_max_bid {
                new_bid = (challenger_id, auction.bid_min);
                new_max_bid = Some((challenger_id, data.amount));
                status_msg = Some(
                    format!(
                        "You have placed the first bid of {amount}{ty}, and set your max bid to {max}{ty}",
                        ty = auction.bid_ty,
                        amount = auction.bid_min,
                        max = data.amount,
                    )
                )
            } else {
                new_bid = (challenger_id, data.amount);
                new_max_bid = None;
                status_msg = Some(
                    format!(
                        "You have placed the first bid of {amount}{ty}",
                        ty = auction.bid_ty,
                        amount = data.amount,
                    )
                )
            }
        } else {
            status_msg = Some("Your bid is below the minimum bid amount".to_string());
            return Ok(());
        }

        if maybe_old_bid != Some(new_bid) {
            if let Some(old_bid) = maybe_old_bid {
                // refund old bidder
                let t = TransactionBuilder::new(old_bid.1, auction.bid_ty.clone(), now)
                    .auction_refund(old_bid.0, &auction);
                match handle.transfer(t) {
                    Err(TransferError::NotEnough) => unreachable!("It is nonsensical for an addition of funds to lack sufficient balance."),
                    Err(TransferError::Overflow) => panic!("The consortium has reached a state such that more than 2^63 fungibles exist, and this code was not designed to handle that"),
                    Ok(v) => v?,
                }
            }
            // charge new bidder
            let t = TransactionBuilder::new(new_bid.1, auction.bid_ty.clone(), now)
                .auction_reserve(new_bid.0, &auction);
            match handle.transfer(t) {
                Err(TransferError::NotEnough) => unreachable!("We have a lock on transactions for this user, and already checked they have enough fungibles."),
                Err(TransferError::Overflow) => panic!("The consortium has reached a state such that more than 2^63 fungibles exist, and this code was not designed to handle that"),
                Ok(v) => v?,
            }
        }

        // The timer is bumped iff the winning bidder changed
        let new_timer_bumped = if maybe_old_bid.map(|b| b.0) != Some(new_bid.0) {
            now
        } else { auction.last_timer_bump };

        diesel::update(adsl::auctions).set((
            adsl::last_timer_bump.eq(new_timer_bumped),
            adsl::max_bid_user.eq(new_max_bid.map(|a| a.0)),
            adsl::max_bid_amt.eq(new_max_bid.map(|a| a.1)),
        )).execute(&*ctx.conn).unwrap();
        
        Ok(())
    });

    match transaction_res {
        Ok(()) => (),
        Err(diesel::result::Error::RollbackTransaction) => (),
        e => e.unwrap(),
    }

    if let Some(status_msg) = status_msg {
        page(
            &mut ctx,
            PageTitle("Auction bid"),
            CanonicalUrl(None),
            html!{},
            html!{
                main { (status_msg) }
                br;
                a href={"/auctions/" (damm_id)} { "Return to auction" }
                br;
                a href="/" { "Return home" }
            }
        )
    } else {
        res.unwrap()
    }
}

#[get("/auctions/<damm_id>")]
pub fn auction_view(
    damm_id: String,
    mut ctx: CommonContext,
) -> PlutoResponse {
    let id:i64 = if let Some(digits) = crate::damm::validate_ascii(damm_id.as_str()) {
        atoi::atoi(digits.as_slice()).unwrap()
    } else {
        return not_found();
    };
    use crate::models::AuctionWinner;
    use crate::view_schema::auction_and_winner::dsl as anw;
    use crate::schema::transfers::dsl as tdsl;

    let maybe_auction:Option<AuctionWinner> = anw::auction_and_winner
    .select(AuctionWinner::cols())
    .filter(anw::auction_id.eq(id))
    .get_result(&*ctx)
    .optional()
    .unwrap();

    let auction = if let Some(a) = maybe_auction {
        a
    } else {
        return not_found();
    };
    let transaction_history:Vec<Transfer> = tdsl::transfers
        .select(Transfer::cols())
        .filter(tdsl::auction_id.eq(auction.auction_id))
        .order((tdsl::happened_at.asc(), tdsl::rowid.asc()))
        .get_results(&*ctx)
        .unwrap();
    
    let mut auction_history:Vec<(DateTime<Utc>, String)> = vec![];
    auction_history.push((
        auction.created_at,
        format!(
            "Auction created by {}",
            auction.auctioneer_name(),
        )
    ));
    for t in &transaction_history {
        match &t.extra {
            TransferExtra::AuctionCreate{ auction_id: _, from: _} => (), // Already covered above
            TransferExtra::AuctionReserve{auction_id: _, from} => {
                auction_history.push((t.happened_at, format!(
                    "{} bids {} {}",
                    name_of(from.discord_id()),
                    t.quantity,
                    auction.bid_ty,
                )))
            },
            TransferExtra::AuctionRefund{ auction_id: _, to: _} => (), // We don't need to show anything, as this always happens at the same instant as an AuctionReserve
            TransferExtra::AuctionPayout{ auction_id: _, to} => {
                auction_history.push(
                    (t.happened_at, format!(
                        "{} wins the auction, receiving {} {}.",
                        name_of(to.discord_id()),
                        t.quantity,
                        auction.offer_ty,
                    ))
                )
            },
            _ => unreachable!(),
        }
    }

    let content = html!{
        main {
            (display_auction(&auction))
            @if !auction.finished {
                @if let Some(ref deets) = ctx.deets {
                    form action={"/auctions/" (damm_id) "/bid"} method="post" {
                        input type="hidden" name="csrf" value=(ctx.csrf_token.clone());
                        label {
                            "Bid "
                            input type="number" name="amount" min=(auction.current_min_bid()) value=(auction.current_min_bid());
                            (auction.bid_ty)
                        }
                        br;
                        select name="is_max_bid" {
                            option value="false" selected { "Actually bid" }
                            option value="true" { "Set my maximum bid" }
                        }
                        br;
                        button type="submit" { "Place bid" }
                    }
                    details {
                        summary { "Tap to show max bid information" }
                        @if auction.max_bid_user == Some(deets.id()) {
                            "Your max bid is "
                            (auction.max_bid_amt.unwrap())
                            " "
                            (auction.bid_ty)
                            "."
                        } @else {
                            "You have no max bid set."
                        }
                    }
                } @else {
                    div { "Log in to bid" }
                }
            }
            h2 { "Auction history" }
            table.tabley-table {
                tr {
                    th { "At" }
                    th {}
                }
                @for (happened_at, msg) in auction_history {
                    tr {
                        td {
                            (show_ts(happened_at))
                        }
                        td{ (msg) }
                    }
                }
            }
        }
    };

    let meta_title = format!(
        "Auction#{} for {} {} @ CONsortium MAS",
        auction.damm(),
        auction.offer_amt,
        auction.offer_ty,
    );

    let meta_description = if auction.finished {
        if let Some(winner) = auction.winner() {
            format!(
                "Auction won by {} paying {} {}.",
                crate::names::name_of(winner.0),
                winner.1,
                auction.bid_ty,
            )
        } else {
            format!(
                "Auction ended with no bids; Minimum bid was {} {}",
                auction.bid_min,
                auction.bid_ty,
            )
        }
    } else if let Some(winner) = auction.winner() {
        format!(
            "Current bid is {} {} by {}",
            winner.1,
            auction.bid_ty,
            crate::names::name_of(winner.0),
        )
    } else {
        format!(
            "No bids; Minimum bid is {} {}",
            auction.bid_min,
            auction.bid_ty,
        )
    };

    page(
        &mut ctx,
        PageTitle(format!("Auction#{}",damm_id)),
        full_url(uri!(auction_view: damm_id = &damm_id)).into(),
        html!{
            meta property="og:title" content=(meta_title);
            meta property="og:description" content=(meta_description);
            meta property="og:type" content="website";
            meta property="og:image" content=(super::statics::static_path!(favicon.png)); //TODO: Autogenerate informational icon
            meta property="og:image:alt" content="Cube inside a large C";
            meta property="og:url" content=(full_url(uri!(auction_view: damm_id = &damm_id)));
            meta property="og:site_name" content="CONsortium MAS";

            meta name="twitter:card" content="summary";

            link rel="index" href=(uri!(auction_index));
        },
        content
    )
}

#[get("/auctions")]
pub fn auction_index(
    mut ctx: CommonContext,
) -> PlutoResponse {
    use crate::view_schema::auction_and_winner::dsl as anw;
    let pending_auctions:Vec<AuctionWinner> =
        anw::auction_and_winner
        .select(AuctionWinner::cols())
        .filter(anw::finished.eq(false))
        .order((
            anw::created_at.desc(),
        ))
        .get_results(&*ctx)
        .unwrap()
    ;
    let finished_auctions:Vec<AuctionWinner> =
        anw::auction_and_winner
        .select(AuctionWinner::cols())
        .filter(anw::finished.eq(true))
        .order((
            anw::created_at.desc(),
        ))
        .get_results(&*ctx)
        .unwrap()
    ;
    page(
        &mut ctx,
        PageTitle("Auctions"),
        full_url(uri!(auction_index)).into(),
        html!{},
        html!{
            h1 { "Auctions" }
            h2 { "Pending auctions" }
            @for auction in pending_auctions {
                article { (display_auction(&auction)) }
            }

            hr;
            h2 { "Finished auctions" }
            @for auction in finished_auctions {
                article { (display_auction(&auction)) }
            }
        }
    )
}