use std::sync::Arc;
use serenity::framework::standard::CommandResult;
use serenity::http::CacheHttp;
use diesel::prelude::*;
use tokio_diesel::{AsyncRunQueryDsl,AsyncConnection,OptionalExtension};
use chrono::TimeZone;
use crate::damm;
use crate::schema;
use crate::view_schema;
use crate::bot;
use crate::bot::DbPool;
use crate::is_win::is_win;
use crate::models::TransferType;

pub async fn create_auto_auctions(
    pool: &Arc<DbPool>,
    cnh: &impl CacheHttp,
) -> CommandResult {
    use diesel::prelude::*;
    use schema::single::dsl as sdsl;
    use schema::auctions::dsl as adsl; //asymmetric digital subscriber line

    let now = chrono::Utc::now();
    let maybe_last_auction:Option<chrono::DateTime<chrono::Utc>> = sdsl::single.select(sdsl::last_auto_auction).get_result_async(pool).await?;
    if let Some(last_auction) = maybe_last_auction {
        let mut next_auction = chrono_tz::US::Pacific.from_utc_datetime(&last_auction.naive_utc());
        next_auction = next_auction + *crate::AUTO_AUCTION_EVERY;
        next_auction = next_auction.date().and_time(*crate::AUTO_AUCTION_AT).unwrap();

        if now > next_auction {
            let auction_id:i64 = pool.transaction(|conn| {
                let auction_id:i64 = diesel::insert_into(adsl::auctions).values((
                    adsl::created_at.eq(chrono::Utc::now()),
                    adsl::auctioneer.eq(None:Option<i64>),
                    adsl::offer_ty.eq("gen"),
                    adsl::offer_amt.eq(1i32),
                    adsl::bid_ty.eq("pc"),
                    adsl::bid_min.eq(1i32),
                ))
                .returning(adsl::rowid)
                .get_result(conn)?;
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
    use schema::auctions::dsl as adsl; //asymmetric digital subscriber line
    use schema::transfers::dsl as tdsl;

    let now = chrono::Utc::now();

    #[derive(Debug, Clone, Queryable)]
    struct Auction {
        pub rowid:i64,
        pub created_at:chrono::DateTime<chrono::Utc>,
        pub offer_ty:String,
        pub offer_amt:i32,
    }

    let auctions_needing_processing:Vec<Auction> = adsl::auctions
    .select((
        adsl::rowid,
        adsl::created_at,
        adsl::offer_ty,
        adsl::offer_amt,
    ))
    .filter(adsl::finished.eq(false))
    .get_results_async(pool).await?;

    for auction in auctions_needing_processing {
        let last_bid:Option<(chrono::DateTime<chrono::Utc>,Option<i64>)> = tdsl::transfers
        .select((tdsl::happened_at,tdsl::from_user))
        .filter(tdsl::auction_id.eq(auction.rowid))
        .filter(tdsl::transfer_ty.eq(TransferType::AuctionReserve))
        .order(tdsl::happened_at.desc())
        .limit(1)
        .get_result_async(pool)
        .await
        .optional()?;
        let last_action_time = last_bid.map(|(time,_)| time).unwrap_or(auction.created_at);
        let finishes_at = last_action_time + *crate::AUCTION_EXPIRATION;
        if finishes_at < now {
            if let Some(user_id) = last_bid.map(|(_,user)| user.unwrap()) {
                pool.transaction(|conn| {
                    use view_schema::balance_history::dsl as bh;

                    let prev_balance:i64 = view_schema::balance_history::table
                    .select(bh::balance)
                    .filter(bh::user.eq(user_id))
                    .filter(bh::ty.eq(auction.offer_ty.as_str()))
                    .order(bh::happened_at.desc())
                    .limit(1)
                    .for_update()
                    .get_result(conn)
                    .optional()?
                    .unwrap_or(0);
                    
                    diesel::insert_into(tdsl::transfers).values((
                        tdsl::quantity.eq(auction.offer_amt as i64),
                        tdsl::to_user.eq(user_id),
                        tdsl::to_balance.eq(prev_balance + (auction.offer_amt as i64)),
                        tdsl::happened_at.eq(chrono::Utc::now()),
                        tdsl::ty.eq(auction.offer_ty.as_str()),
                        tdsl::transfer_ty.eq(TransferType::AuctionPayout),
                    )).execute(conn)?;

                    diesel::update(adsl::auctions.filter(adsl::rowid.eq(auction.rowid))).set(adsl::finished.eq(true)).execute(conn)?;
            
                    Ok(())
                }).await?;

                serenity::model::id::ChannelId::from(bot::MOTIONS_CHANNEL).send_message(cnh.http(), |m| {
                    use serenity::prelude::Mentionable;
                    m.content(format!(
                        "Auction#{0} finished. {2} received {3} {4}. Visit <{1}/auctions/{0}> for more details.",
                        damm::add_to_str(auction.rowid.to_string()),
                        crate::SITE_URL,
                        serenity::model::id::UserId::from(user_id as u64).mention(),
                        auction.offer_amt,
                        auction.offer_ty.as_str(),
                    ))
                }).await?;
            } else {
                diesel::update(adsl::auctions.filter(adsl::rowid.eq(auction.rowid))).set(adsl::finished.eq(true)).execute_async(pool).await?;
                serenity::model::id::ChannelId::from(bot::MOTIONS_CHANNEL).send_message(cnh.http(), |m| {
                    m.content(format!(
                        "Auction#{0} finished. There were no bids, no one gets anything. Visit <{1}/auctions/{0}> for no details.",
                        damm::add_to_str(auction.rowid.to_string()),
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
    // use schema::gen::dsl as gdsl;
    use schema::transfers::dsl as tdsl;
    use diesel::prelude::*;
    use view_schema::balance_history::dsl as bhdsl;
    use schema::single::dsl as sdsl;
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

        let users:Vec<Option<i64>> = tdsl::transfers.select(tdsl::to_user).distinct().filter(tdsl::ty.eq("gen")).filter(tdsl::to_user.is_not_null()).get_results(&*conn)?;
        for userid_o in &users {
            let userid = userid_o.unwrap();
            let balance = |ty_str:&'static str| {
                Ok(bhdsl::balance_history
                    .select(bhdsl::balance)
                    .filter(bhdsl::user.eq(userid))
                    .filter(bhdsl::ty.eq(ty_str))
                    .filter(bhdsl::happened_at.lt(this_gen)) //This ensures that a late generator run will still give pc to the owner of the gen at the time it was supposed to pay out
                    .order(bhdsl::happened_at.desc())
                    .limit(1)
                    .get_result(&*conn)
                    .optional()?
                    .unwrap_or(0)):Result<i64,diesel::result::Error>
            };
            let gen_balance = balance("gen")?;
            let pc_balance = balance("pc")?;
            diesel::insert_into(tdsl::transfers).values((
                tdsl::ty.eq("pc"),
                tdsl::quantity.eq(gen_balance),
                tdsl::to_user.eq(userid),
                tdsl::to_balance.eq(pc_balance + gen_balance),
                tdsl::happened_at.eq(now),
                tdsl::transfer_ty.eq(TransferType::Generated),
            )).execute(&*conn)?;
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