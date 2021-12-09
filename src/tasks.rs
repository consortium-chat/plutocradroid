use std::sync::Arc;
use serenity::framework::standard::CommandResult;
use serenity::http::CacheHttp;
use diesel::prelude::*;
use tokio_diesel::{AsyncRunQueryDsl,AsyncConnection};
use chrono::{Utc,TimeZone};
use crate::damm;
use crate::schema;
use crate::view_schema;
use crate::bot;
use crate::bot::DbPool;
use crate::is_win::is_win;

pub async fn create_auto_auctions(
    pool: &Arc<DbPool>,
    cnh: &impl CacheHttp,
) -> CommandResult {
    use diesel::prelude::*;
    use schema::single::dsl as sdsl;
    use schema::auctions::dsl as adsl; //asymmetric digital subscriber line
    use schema::thing_ids::dsl as tid;

    let now = chrono::Utc::now();
    let maybe_last_auction:Option<chrono::DateTime<chrono::Utc>> = sdsl::single.select(sdsl::last_auto_auction).get_result_async(pool).await?;
    if let Some(last_auction) = maybe_last_auction {
        let mut next_auction = chrono_tz::US::Pacific.from_utc_datetime(&last_auction.naive_utc());
        next_auction = next_auction + *crate::AUTO_AUCTION_EVERY;
        next_auction = next_auction.date().and_time(*crate::AUTO_AUCTION_AT).unwrap();

        if now > next_auction {
            let now = Utc::now();
            let auction_id:i64 = pool.transaction(|conn| {
                let auction_id:i64 = diesel::insert_into(tid::thing_ids).default_values().returning(tid::rowid).get_result(conn)?;
                diesel::insert_into(adsl::auctions).values((
                    adsl::rowid.eq(auction_id),
                    adsl::created_at.eq(now),
                    adsl::auctioneer.eq(None:Option<i64>),
                    adsl::offer_ty.eq("gen"),
                    adsl::offer_amt.eq(1i64),
                    adsl::bid_ty.eq("pc"),
                    adsl::bid_min.eq(1i64),
                    adsl::last_timer_bump.eq(now),
                ))
                .execute(conn)?;
                diesel::update(sdsl::single).set(sdsl::last_auto_auction.eq(next_auction)).execute(conn)?;
                Ok(auction_id)
            }).await?;

            serenity::model::id::ChannelId::from(bot::MOTIONS_CHANNEL).send_message(cnh.http(), |m| {
                m.content(format!(
                    "New auction#{0} started! The Consortium offers 1 gen for bids in pc. Visit <{1}/auctions/{0}> to bid.",
                    damm::add_to_str(auction_id.to_string()),
                    crate::SITE_URL,
                ))
            }).await?;
        }
    }
    Ok(())
}

pub async fn process_auctions(
    pool: &Arc<DbPool>,
    cnh: &impl CacheHttp,
) -> CommandResult {
    use diesel::prelude::*;
    use view_schema::auction_and_winner::dsl as anw;
    use schema::auctions::dsl as adsl; //asymmetric digital subscriber line

    let now = chrono::Utc::now();

    use crate::models::AuctionWinner;

    let auctions_needing_processing:Vec<AuctionWinner> = anw::auction_and_winner
        .select(AuctionWinner::cols())
        .filter(anw::finished.eq(false))
        .get_results_async(pool).await?;

    for auction in auctions_needing_processing {
        let finishes_at = auction.last_timer_bump + *crate::AUCTION_EXPIRATION;
        if finishes_at < now {
            if let Some(user_id) = auction.winner_id {
                pool.transaction(|conn| {
                    let mut handle = crate::transfers::TransferHandler::new(
                        conn,
                        vec![user_id],
                        vec![auction.offer_ty.clone()],
                    )?;

                    let t = crate::transfers::TransactionBuilder::new(
                        auction.offer_amt,
                        auction.offer_ty.clone(),
                        now,
                    ).auction_payout(
                        user_id,
                        &auction,
                    );

                    handle.transfer(t).unwrap()?;
                    diesel::update(
                        adsl::auctions.filter(
                            adsl::rowid.eq(auction.auction_id)
                        )
                    ).set(adsl::finished.eq(true)).execute(conn)?;
            
                    Ok(())
                }).await?;

                serenity::model::id::ChannelId::from(bot::MOTIONS_CHANNEL).send_message(cnh.http(), |m| {
                    use serenity::prelude::Mentionable;
                    m.content(format!(
                        "Auction#{0} finished. {2} received {3} {4}. Visit <{1}/auctions/{0}> for more details.",
                        damm::add_to_str(auction.auction_id.to_string()),
                        crate::SITE_URL,
                        user_id.into_serenity().mention(),
                        auction.offer_amt,
                        auction.offer_ty.as_str(),
                    ))
                }).await?;
            } else {
                diesel::update(adsl::auctions.filter(adsl::rowid.eq(auction.auction_id))).set(adsl::finished.eq(true)).execute_async(pool).await?;
                serenity::model::id::ChannelId::from(bot::MOTIONS_CHANNEL).send_message(cnh.http(), |m| {
                    m.content(format!(
                        "Auction#{0} finished. There were no bids, no one gets anything. Visit <{1}/auctions/{0}> for no details.",
                        damm::add_to_str(auction.auction_id.to_string()),
                        crate::SITE_URL,
                    ))
                }).await?;
            }
        }
    }

    Ok(())
}

pub fn process_generators(
    conn: &diesel::PgConnection
) -> Result<bool, diesel::result::Error> {
    use diesel::prelude::*;
    use crate::schema::transfers::dsl as tdsl;
    use crate::schema::single::dsl as sdsl;
    use crate::models::UserId;
    use crate::transfers::{CurrencyId,TransactionBuilder,TransferHandler};
    let now = chrono::Utc::now();
    let last_gen:chrono::DateTime<chrono::Utc> = sdsl::single.select(sdsl::last_gen).get_result(&*conn)?;

    if now - last_gen < *crate::GENERATE_EVERY {
        return Ok(false);
    }
    let this_gen = last_gen + *crate::GENERATE_EVERY;
    eprintln!("Generating some political capital!");
    let start_chrono = chrono::Utc::now();
    let start_instant = std::time::Instant::now();
    conn.transaction::<_, diesel::result::Error, _>(|| {
        diesel::sql_query("LOCK TABLE transfers IN EXCLUSIVE MODE;").execute(&*conn)?;

        let users:Vec<Option<UserId>> = tdsl::transfers.select(tdsl::to_user).distinct().filter(tdsl::ty.eq(CurrencyId::GEN)).filter(tdsl::to_user.is_not_null()).get_results(&*conn)?;
        for userid_o in &users {
            let userid = userid_o.unwrap();

            let mut handle = TransferHandler::new(
                &*conn,
                vec![userid],
                vec![CurrencyId::PC, CurrencyId::GEN],
            )?;

            let gen_balance = handle.balance(userid, CurrencyId::GEN);
            let t = TransactionBuilder::new(
                gen_balance,
                CurrencyId::PC,
                now,
            ).fabricate(userid, true);
            handle.transfer(t).unwrap()?;
        }

        diesel::update(sdsl::single).set(sdsl::last_gen.eq(this_gen)).execute(&*conn)?;
        
        Ok(())
    })?;
    let end_instant = std::time::Instant::now();
    let end_chrono = chrono::Utc::now();
    let chrono_dur = end_chrono - start_chrono;

    eprintln!("PC generation took {} kernel seconds/{} RTC seconds", (end_instant - start_instant).as_secs_f64(), chrono_dur);
    Ok(true)
}

pub async fn process_motion_completions(
    pool: &Arc<DbPool>,
    cnh: &impl CacheHttp,
) -> CommandResult {
    use diesel::prelude::*;
    use schema::motions::dsl as mdsl;
    use schema::motion_votes::dsl as mvdsl;
    let now = chrono::Utc::now();
    let motions:Vec<(String, i64, bool)> = mdsl::motions
        .filter(mdsl::announcement_message_id.is_null())
        .filter(mdsl::last_result_change.lt(now - *crate::MOTION_EXPIRATION))
        .select((mdsl::motion_text, mdsl::rowid, mdsl::is_super))
        .get_results_async(&pool).await?;
    for (motion_text, motion_id, is_super) in &motions {
        #[derive(Queryable,Debug)]
        struct MotionVote {
            amount:i64,
            direction:bool,
        }
        let votes:Vec<MotionVote> = mvdsl::motion_votes
            .select((mvdsl::amount, mvdsl::direction))
            .filter(mvdsl::motion.eq(motion_id))
            .get_results_async(&pool).await?;
        let mut yes_votes = 0;
        let mut no_votes = 0;
        for vote in &votes {
            if vote.direction {
                yes_votes += vote.amount;
            } else {
                no_votes += vote.amount;
            }
        }
        let pass = is_win(yes_votes, no_votes, *is_super);
        let pass_msg = if pass { "PASSED" } else { "FAILED" }; 
        let announce_msg = serenity::model::id::ChannelId::from(bot::MOTIONS_CHANNEL).send_message(cnh.http(), |m| {
            m.embed(|e| {
                e.title(
                    format!(
                        "Vote ended! Motion #{} has {}.",
                        damm::add_to_str(motion_id.to_string()), 
                        pass_msg,
                    )
                );
                if pass { e.description(motion_text); }
                e.timestamp(&now);
                if pass {
                    e.field("Votes", format!("**for {}**/{} against", yes_votes, no_votes), false);
                }else{
                    e.field("Votes", format!("**against {}**/{} for", no_votes, yes_votes), false);
                }
                e
            })
        }).await?;

        diesel::update(mdsl::motions.filter(mdsl::rowid.eq(motion_id))).set(
            mdsl::announcement_message_id.eq(announce_msg.id.0 as i64)
        ).execute_async(&pool).await?;
    }

    let mmids:Vec<i64> = mdsl::motions
        .filter(mdsl::announcement_message_id.is_null())
        .filter(mdsl::needs_update)
        .select(mdsl::bot_message_id)
        .get_results_async(&pool).await?;
    for mmid in &mmids {
        let mut motion_message = cnh.http().get_message(bot::MOTIONS_CHANNEL, *mmid as u64).await?;
        bot::update_motion_message(cnh, Arc::clone(&pool), &mut motion_message).await?;
    }
    Ok(())
}

pub fn update_last_task_run(
    conn: &diesel::PgConnection
) -> Result<(), diesel::result::Error> {
    use schema::single::dsl as sdsl;
    diesel::update(sdsl::single).set(sdsl::last_task_run.eq(diesel::dsl::now)).execute(conn)?;
    Ok(())
}