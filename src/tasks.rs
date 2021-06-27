use std::sync::Arc;
use serenity::framework::standard::CommandResult;
use serenity::http::CacheHttp;
use serenity::framework::standard::CommandError;
use crate::damm;
use crate::schema;
use crate::view_schema;
use crate::bot;
use crate::bot::DbPool;
use crate::is_win::is_win;

pub async fn process_generators(
    pool: Arc<DbPool>
) -> Result<bool,CommandError> {
    // use schema::gen::dsl as gdsl;
    use schema::transfers::dsl as tdsl;
    use diesel::prelude::*;
    use view_schema::balance_history::dsl as bhdsl;
    use schema::single::dsl as sdsl;
    let now = chrono::Utc::now();
    let conn = pool.get()?;
    let last_gen:chrono::DateTime<chrono::Utc> = sdsl::single.select(sdsl::last_gen).get_result(&*conn)?;

    if now - last_gen < *bot::GENERATE_EVERY {
        return Ok(false);
    }
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
                tdsl::transfer_ty.eq("generated"),
            )).execute(&*conn)?;
        }

        diesel::update(sdsl::single).set(sdsl::last_gen.eq(last_gen + *bot::GENERATE_EVERY)).execute(&*conn)?;
        
        Ok(())
    })?;
    let end_instant = std::time::Instant::now();
    let end_chrono = chrono::Utc::now();
    let chrono_dur = end_chrono - start_chrono;

    eprintln!("PC generation took {} kernel seconds/{} RTC seconds", (end_instant - start_instant).as_secs_f64(), chrono_dur);
    Ok(true)
}

pub async fn process_motion_completions(
    pool: Arc<DbPool>,
    cnh: &impl CacheHttp,
) -> CommandResult {
    use diesel::prelude::*;
    use schema::motions::dsl as mdsl;
    use schema::motion_votes::dsl as mvdsl;
    let now = chrono::Utc::now();
    let conn = pool.get()?;
    let motions:Vec<(String, i64, bool)> = mdsl::motions
        .filter(mdsl::announcement_message_id.is_null())
        .filter(mdsl::last_result_change.lt(now - *bot::MOTION_EXPIRATION))
        .select((mdsl::motion_text, mdsl::rowid, mdsl::is_super))
        .get_results(&conn)?;
    for (motion_text, motion_id, is_super) in &motions {
        #[derive(Queryable,Debug)]
        struct MotionVote {
            user:i64,
            amount:i64,
            direction:bool,
        }
        let votes:Vec<MotionVote> = mvdsl::motion_votes
            .filter(mvdsl::motion.eq(motion_id))
            .select((mvdsl::user, mvdsl::amount, mvdsl::direction))
            .get_results(&conn)?;
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
        ).execute(&conn)?;
    }

    let mmids:Vec<i64> = mdsl::motions
        .filter(mdsl::announcement_message_id.is_null())
        .filter(mdsl::needs_update)
        .select(mdsl::bot_message_id)
        .get_results(&*conn)?;
    for mmid in &mmids {
        let mut motion_message = cnh.http().get_message(bot::MOTIONS_CHANNEL, *mmid as u64).await?;
        bot::update_motion_message(cnh, Arc::clone(&pool), &mut motion_message).await?;
    }
    Ok(())
}