use crate::schema;
use crate::view_schema;
use crate::damm;

use std::sync::Arc;
#[allow(unused_imports)]
use std::convert::{TryInto, TryFrom};

use tokio_diesel::*;

use chrono::Utc;

use serenity::client::Client;
use serenity::model::misc::Mentionable;
use serenity::model::channel::Message;
use serenity::model::id::UserId as SerenityUserId;
use serenity::prelude::{EventHandler, Context};
use serenity::http::CacheHttp;
use serenity::framework::standard::{
    StandardFramework,
    CommandResult,
    CommandError,
    DispatchError,
    macros::{
        command,
        group,
        hook
    },
    Args,
};
use regex::Regex;

use diesel::connection::Connection;

use tokio::task;

use async_trait::async_trait;

use bigdecimal::BigDecimal;

use crate::is_win::is_win;
use crate::motion_label::motion_label;
use crate::models::{self, ItemType};
use crate::transfers::{TransferHandler, TransactionBuilder, TransferError, CurrencyId};

pub type DbPool = diesel::r2d2::Pool<diesel::r2d2::ConnectionManager<diesel::PgConnection>>;

struct DbPoolKey;
impl serenity::prelude::TypeMapKey for DbPoolKey {
    type Value = Arc<DbPool>;
}

#[group]
#[commands(ping, give, force_give, balances, motion, supermotion, submotion, vote, hack_message_update, help, version_info)]
struct General;

#[group]
#[commands(fabricate, debug_make_auction)]
struct Debug;

use std::env;

struct Handler;

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
enum SpecialEmojiAction {
    Direction(bool),
    Amount(u64),
}

lazy_static! {
    static ref USER_PING_RE:Regex = Regex::new(r"^\s*<@!?(\d+)>\s*$").unwrap();
    static ref SPECIAL_EMOJI:std::collections::HashMap<u64,SpecialEmojiAction> = hashmap!{
        770749957723783169 => SpecialEmojiAction::Amount(1),
        770750097793089596 => SpecialEmojiAction::Amount(2),
        770750182874021888 => SpecialEmojiAction::Amount(5),
        770750211281780776 => SpecialEmojiAction::Amount(10),
        770750297621921802 => SpecialEmojiAction::Amount(20),
        770750316530499604 => SpecialEmojiAction::Amount(50),
        770750332946874388 => SpecialEmojiAction::Amount(100),
        770750231776198698 => SpecialEmojiAction::Amount(10),
        770749937960747029 => SpecialEmojiAction::Amount(1),
        770750552291410000 => SpecialEmojiAction::Direction(true),
        770750576257531914 => SpecialEmojiAction::Direction(false),
    };
}


pub const VOTE_BASE_COST:u16 = 40;
#[cfg(not(feature = "debug"))]
pub const MOTIONS_CHANNEL:u64 = 983019887024807976; //bureaucracy channel
#[cfg(feature = "debug")]
//const MOTIONS_CHANNEL:u64 = 694013828362534983; //pluto-dev channel
//const MOTIONS_CHANNEL:u64 = 610387757818183690; //test channel in shelvacuisawesomeserver
//const MOTIONS_CHANNEL:u64 = 560918427091468387; //spam channel
pub const MOTIONS_CHANNEL:u64 = 983019887024807976; //pluto-beta-messages in CONceptualization

#[cfg(not(feature = "debug"))]
pub const MY_ID_INT:u64 = 415006970605731844;
#[cfg(feature = "debug")]
pub const MY_ID_INT:u64 = 415006970605731844;

pub const MY_ID:SerenityUserId = SerenityUserId(MY_ID_INT);

#[async_trait]
trait FromCommandArgs : Sized {
    async fn from_command_args(ctx: &Context, msg: &Message, arg: &str) -> Result<Self, &'static str>;
}

#[async_trait]
impl FromCommandArgs for SerenityUserId {
    async fn from_command_args(ctx: &Context, msg: &Message, arg: &str) -> Result<Self, &'static str> {
        trace!("from_command_args");
        if arg == "." || arg == "self" {
            return Ok(msg.author.id);
        }
        if let Ok(raw_id) = arg.parse():Result<u64,_> {
            return Ok(SerenityUserId::from(raw_id));
        }

        if let Some(ma) = USER_PING_RE.captures(arg) {
            if let Ok(raw_id) = ma.get(1).unwrap().as_str().parse():Result<u64,_> {
                return Ok(SerenityUserId::from(raw_id));
            }
        }

        if arg.contains('#') {
            let pieces = arg.rsplitn(2,'#').collect():Vec<&str>;
            if let Ok(discriminator) = pieces[0].parse():Result<u16, _> {
                if discriminator <= 9999 {
                    let name = pieces[1];
                    let users = ctx.cache.users().await;
                    let maybe_user = users
                        .values()
                        .find(|user| {
                            user.discriminator == discriminator && user.name.to_ascii_uppercase() == name.to_ascii_uppercase()
                        });
                    if let Some(user) = maybe_user {
                        return Ok(user.id);
                    }
                }
            }
        }

        for guild_id in ctx.cache.guilds().await {
            if let Some(members) = ctx.cache.guild_field(guild_id, |g| g.members.clone()).await {
                for (userid, member) in members {
                    if let Some(nick) = member.nick.as_ref() {
                        if nick.to_ascii_uppercase() == arg.to_ascii_uppercase() {
                            return Ok(userid);
                        }
                    }
                    if member.user.name.to_ascii_uppercase() == arg.to_ascii_uppercase() {
                        return Ok(userid);
                    }
                }
            }
        }
        Err("Could not find any User.")
    }
}

#[serenity::async_trait]
impl EventHandler for Handler {
    async fn reaction_add(&self, ctx: Context, r: serenity::model::channel::Reaction) {
        trace!("reaction_add");
        let mut vote_count = 0;
        let mut vote_direction = None;
        let maybe_user_id = r.user_id;
        if let Some(user_id) = maybe_user_id {
            if user_id == ctx.cache.current_user_id().await {
                return;
            }
            let message_id = r.message_id;
            if let serenity::model::channel::ReactionType::Custom{animated: _, id, name: _} = r.emoji {
                if let Some(action) = SPECIAL_EMOJI.get(&id.0) {
                    match action {
                        SpecialEmojiAction::Direction(dir) => vote_direction = Some(*dir),
                        SpecialEmojiAction::Amount(a) => vote_count = *a,
                    }
                    let pool = Arc::clone(ctx.data.read().await.get::<DbPoolKey>().unwrap());
                    let resp = vote_common_async(
                        pool,
                        vote_direction,
                        vote_count as i64,
                        user_id.into(),
                        None,
                        Some(message_id.0 as i64),
                        None,
                    ).await;
                    user_id.create_dm_channel(&ctx).await.unwrap().say(&ctx, resp).await.unwrap();
                }
            }
        }
    }
}

fn nth_vote_cost(n:i64) -> Result<i64,()> {
    trace!("nth_vote_cost");
    let res:f64 = (VOTE_BASE_COST as f64) * (1.05f64).powf((n-1) as f64);
    if (0.0..4611686018427388000.0).contains(&res) {
        Ok(res as i64)
    } else {
        Err(())
    }
}

#[hook]
async fn on_dispatch_error_hook(_context: &Context, msg: &Message, error: DispatchError){
    println!(
        "{:?}\nerr'd with {:?}",
        msg, error
    );
}


#[hook]
async fn after_hook(ctx: &Context, msg: &Message, _cmd_name: &str, error: Result<(), CommandError>) {
    trace!("after_hook");
    //  Print out an error if it happened
    if let Err(why) = error {
        let _ = msg.reply(ctx, why).await;
    }
}

pub async fn bot_main() {
    trace!("bot_main begin");
    lazy_static::initialize(&USER_PING_RE);

    let raw_pool = diesel::r2d2::Builder::new().build(
        diesel::r2d2::ConnectionManager::<diesel::PgConnection>::new(
            &env::var("DATABASE_URL").expect("DATABASE_URL expected")
        )
    ).expect("could not build DB pool");
    let arc_pool = Arc::new(raw_pool);

    trace!("built pool");

    {
        use schema::single::dsl::*;
        use diesel::prelude::*;
        use diesel::dsl::*;
        if !(select(exists(single.filter(enforce_single_row))).get_result_async(&*arc_pool).await.unwrap():bool) {
            insert_into(single).values((
                enforce_single_row.eq(true),
                last_gen.eq(chrono::Utc::now())
            )).execute_async(&*arc_pool).await.unwrap();
        }
    }
    
    #[cfg(feature = "debug")]
    let prefix = "&";
    #[cfg(not(feature = "debug"))]
    let prefix = "$";
    let mut framework = StandardFramework::new()
    .configure(|c| {
        c.prefix(prefix).allow_dm(true).on_mention(Some(MY_ID))
    })
    .on_dispatch_error(on_dispatch_error_hook)
    .after(after_hook);
    framework = framework.group(&GENERAL_GROUP);
    #[cfg(feature = "debug")]
    { framework = framework.group(&GENERAL_GROUP).group(&DEBUG_GROUP); }
    trace!("framework configured");


    // Login with a bot token from the environment
    let mut client = {
        use serenity::client::bridge::gateway::GatewayIntents;
        Client::builder(&env::var("DISCORD_TOKEN").expect("token"))
            .event_handler(Handler)
            .framework(framework)
            .intents(
                GatewayIntents::GUILD_MEMBERS |
                GatewayIntents::GUILD_MESSAGES |
                GatewayIntents::GUILD_MESSAGE_REACTIONS |
                GatewayIntents::DIRECT_MESSAGES
            )
            .await
            .expect("Error creating client")
    };
    trace!("Client configured");
    let mut write_handle = client.data.write().await;
    write_handle.insert::<DbPoolKey>(Arc::clone(&arc_pool));
    drop(write_handle);

    #[cfg(not(feature = "debug"))]
    println!("Prod mode.");

    #[cfg(feature = "debug")]
    println!("Debug mode.");

    trace!("about to client.start()");
    // start listening for events by starting a single shard
    if let Err(why) = client.start().await {
        println!("An error occurred while running the client: {:?}", why);
    }
    warn!("end bot_main, should never end");
}

pub async fn update_motion_message(
    cnh: impl CacheHttp,
    pool: Arc<DbPool>,
    msg: &mut serenity::model::channel::Message
) -> CommandResult {
    trace!("update_motion_message");
    use schema::motions::dsl as mdsl;
    use schema::motion_votes::dsl as mvdsl;
    use diesel::prelude::*;
    
    let (motion_text, motion_id, power) = mdsl::motions
        .filter(mdsl::bot_message_id.eq(msg.id.0 as i64))
        .select((mdsl::motion_text, mdsl::rowid, mdsl::power))
        .get_result_async(&*pool)
        .await?: (String, i64, BigDecimal);
    use crate::models::MotionVote;
    let mut votes:Vec<MotionVote> = 
        mvdsl::motion_votes
        .select(MotionVote::cols())
        .filter(mvdsl::motion.eq(motion_id))
        .get_results_async(&*pool).await?;
    let mut yes_votes = 0;
    let mut no_votes = 0;
    for vote in &votes {
        if vote.direction {
            yes_votes += vote.amount;
        } else {
            no_votes += vote.amount;
        }
    }
    votes.sort_unstable_by_key(|v| -v.amount);
    let pass = is_win(yes_votes, no_votes, &power);
    let cap_label = motion_label(&power);
    msg.edit(cnh, |m| {
        m.embed(|e| {
            e.field(cap_label, motion_text, false);
            if pass {
                e.field("Votes", format!("**for {}**/{} against", yes_votes, no_votes), false);
            } else {
                e.field("Votes", format!("**against {}**/{} for", no_votes, yes_votes), false);
            }
            for vote in &votes[0..std::cmp::min(votes.len(),21)] {
                e.field(crate::names::name_of(vote.user.into_serenity()), format!("{} {}", vote.amount, if vote.direction {"for"} else {"against"}), true);
            }

            if votes.len() > 21 {
                e.field("Note", "There are more users that have voted, but there are too many to display here.", false);
            }
            e
        })
    }).await?;
    let target = mdsl::motions.filter(mdsl::bot_message_id.eq(msg.id.0 as i64));
    diesel::update(target).set(mdsl::needs_update.eq(false)).execute_async(&*pool).await?;
    Ok(())
}

#[command]
#[num_args(1)]
async fn hack_message_update(ctx: &Context, _msg: &Message, mut args: Args) -> CommandResult {
    trace!("hack_message_update");
    let motion_message_id:u64 = args.single()?;
    let mut motion_message = ctx.http.get_message(MOTIONS_CHANNEL, motion_message_id).await?;
    update_motion_message(ctx, Arc::clone(ctx.data.read().await.get::<DbPoolKey>().unwrap()), &mut motion_message).await
}

#[command]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    trace!("ping");
    msg.reply(ctx, "The use of such childish terminology to describe a professional sport played in the olympics such as table tennis is downright offensive to the athletes that have dedicated their lives to perfecting the art. Furthermore, usage of the sport as some inane way to check presence in computer networks and programs would imply that anyone can return a serve as long as they're present, which further degredates the athletes that work day and night to compete for championship tournaments throughout the world.\n\nIn response to your *serve*, I hit back a full force spinball corner return. Don't even try to hit it back.").await?;

    Ok(())
}

// Use like &debug_make_auction 10 gen 1 pc
// to create an auction offering 10 gens at a minimum bid of 1 pc
#[command]
#[num_args(4)]
async fn debug_make_auction(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    use diesel::prelude::*;
    use schema::auctions::dsl as adsl;
    use schema::thing_ids::dsl as tid;
    let now = Utc::now();
    let pool = Arc::clone(ctx.data.read().await.get::<DbPoolKey>().unwrap());

    let offer_amt:i64 = args.single()?;
    if offer_amt < 1 { return Err("fuck".into()); }
    let offer_ty = find_item_type(&*pool, args.single()?).await?;

    let min_bid_amt:i64 = args.single()?;
    if min_bid_amt < 1 { return Err("fuck".into()); }
    let bid_ty = find_item_type(&*pool, args.single()?).await?;
    let auction_id:i64 = diesel::insert_into(tid::thing_ids).default_values().returning(tid::rowid).get_result_async(&*pool).await?;

    diesel::insert_into(adsl::auctions).values((
        adsl::rowid.eq(auction_id),
        adsl::created_at.eq(now),
        adsl::offer_ty.eq(offer_ty.id),
        adsl::offer_amt.eq(offer_amt),
        adsl::bid_ty.eq(bid_ty.id),
        adsl::bid_min.eq(min_bid_amt),
        adsl::last_timer_bump.eq(now),
    )).execute_async(&*pool).await?;

    let auction_damm_id = damm::add_to_str(auction_id.to_string());

    msg.reply(&ctx, format!(
        "Create auction#{} auctioning {} {} for a minimum of {} {}",
        auction_damm_id,
        offer_amt,
        offer_ty.long_name_ambiguous,
        min_bid_amt,
        bid_ty.long_name_ambiguous,
    )).await?;

    Ok(())
}

#[command]
#[num_args(2)]
async fn fabricate(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let now = Utc::now();
    trace!("fabricate");
    let pool = Arc::clone(ctx.data.read().await.get::<DbPoolKey>().unwrap());

    let ty_str:String = args.single()?;
    let ty = find_item_type(&*pool, ty_str).await?;
    let how_many:i64 = args.single()?;
    if how_many <= 0 {
        return Err("fuck".into());
    }
    let user:SerenityUserId = if args.remaining() > 0 {
        let user_str = args.single()?:String;
        SerenityUserId::from_command_args(ctx, msg, &user_str).await?
    }else{
        msg.author.id
    };

    pool.transaction(|txn| {
        let mut handle = TransferHandler::new(
            txn,
            vec![user.into()],
            vec![ty.id.clone()]
        )?;
        let t = TransactionBuilder::new(
            how_many,
            ty.id.clone(),
            now,
        ).fabricate(user.into(), false).message_id(msg.id);
        handle.transfer(t).unwrap()
    }).await?;

    msg.reply(&ctx, "Fabricated.").await?;

    Ok(())
}

#[command]
#[aliases("?","h")]
async fn help(ctx: &Context, msg: &Message) -> CommandResult {
    trace!("help");
    msg.reply(&ctx, "For help see https://github.com/consortium-chat/plutocradroid/blob/master/README.md#commands").await?;
    Ok(())
}

#[command]
#[aliases("v","info","version")]
async fn version_info(ctx: &Context, msg: &Message) -> CommandResult {
    trace!("version_info");
    msg.reply(&ctx, format!(
        "Plutocradroid {} commit {} built for {} at {}.\nhttps://github.com/consortium-chat/plutocradroid",
        env!("VERGEN_BUILD_SEMVER"),
        env!("VERGEN_GIT_SHA"),
        env!("VERGEN_CARGO_TARGET_TRIPLE"),
        env!("VERGEN_BUILD_TIMESTAMP"),
    )).await?;
    Ok(())
}

#[command]
#[aliases("b","bal","balance","i","inv","inventory")]
async fn balances(ctx: &Context, msg: &Message) -> CommandResult {
    trace!("balances");
    use diesel::prelude::*;
    use view_schema::balance_history::dsl as bh;
    use schema::item_types::dsl as it;
    

    let pool = Arc::clone(ctx.data.read().await.get::<DbPoolKey>().unwrap());
    let item_types:Vec<ItemType> = it::item_types
        .select(ItemType::cols())
        .order(it::position)
        .get_results_async(&*pool).await?;
    
    let mut balances = Vec::new();
    for it in item_types {
        let bal = bh::balance_history
        .select(bh::balance)
        .filter(bh::user.eq(msg.author.id.0 as i64))
        .filter(bh::ty.eq(it.db_name()))
        .order((bh::happened_at.desc(), bh::rowid.desc(), bh::sign.desc()))
        .limit(1)
        .get_result_async(&*pool)
        .await
        .optional()
        .map(|opt| opt.unwrap_or(0i64))?:i64;

        balances.push((it, bal));
    };
    msg.channel_id.send_message(&ctx, |cm| {
        cm.embed(|e| {
            e.title("Your balances:");
            for (item_type, amount) in &balances {
                e.field(&item_type.long_name_plural, amount, false);
            }
            e
        });
        cm
    }).await?;
    Ok(())
}


#[command]
#[min_args(2)]
#[max_args(3)]
async fn give(ctx:&Context, msg:&Message, args:Args) -> CommandResult {
    give_common(ctx, msg, args, true).await
}

#[command]
#[min_args(2)]
#[max_args(3)]
async fn force_give(ctx:&Context, msg:&Message, args:Args) -> CommandResult {
    give_common(ctx, msg, args, false).await
}

async fn give_common(ctx:&Context, msg:&Message, mut args:Args, check_user:bool) -> CommandResult {
    trace!("give_common");
    use diesel::prelude::*;
    use schema::item_types::dsl as it;
    use schema::item_type_aliases::dsl as ita;
    let now = Utc::now();
    let pool = Arc::clone(ctx.data.read().await.get::<DbPoolKey>().unwrap());

    let user_str:String = args.single()?;
    let user = SerenityUserId::from_command_args( ctx, msg, &user_str ).await?;
    let user_in_guild = if let Some(guild_id) = msg.guild_id {
        match guild_id.member(ctx,user).await {
            Ok(_) => true,
            Err(serenity::prelude::SerenityError::Http(_)) => false,
            Err(e) => return Err(Box::new(e)),
        }
    } else { false };
    if check_user && !ctx.cache.users().await.contains_key(&user) && !user_in_guild {
        return Err("User not found".into());
    }
    let mut maybe_ty:Option<ItemType> = None;
    let mut amount:Option<i64> = None;
    for arg_result in args.iter::<String>(){
        let arg = arg_result.unwrap();
        let alias:Option<ItemType> = ita::item_type_aliases
            .inner_join(it::item_types)
            .select(ItemType::cols())
            .filter(ita::alias.eq(&arg))
            .get_result_async(&*pool)
            .await
            .optional()?;
        if let Some(ty) = alias {
            maybe_ty = Some(ty);
        } else if let Some(idx) = arg.find(|c| !('0'..='9').contains(&c)) {
            if idx == 0 {
                return Err(format!("Invalid item type {}", arg).into());
            }
            let (count_str, ty_str) = arg.split_at(idx);
            if !ty_str.is_empty() {
                let alias:Option<ItemType> = ita::item_type_aliases
                    .inner_join(it::item_types)
                    .select(ItemType::cols())
                    .filter(ita::alias.eq(&ty_str))
                    .get_result_async(&*pool)
                    .await
                    .optional()?;
                if let Some(ty) = alias {
                    maybe_ty = Some(ty);
                } else {
                    return Err(format!("Unrecognized item type {}", ty_str).into());
                }
            }

            match count_str.parse():Result<i64,_> {
                Err(e) => return Err(format!("Bad count {:?}", e).into()),
                Ok(val) if val < 0 => return Err("No negatives >:(".into()),
                Ok(val) => amount = Some(val),
            }
        }else{
            match arg.parse():Result<i64, _> {
                Err(e) => return Err(format!("Bad count {:?}", e).into()),
                Ok(val) if val < 0 => return Err("No negatives >:(".into()),
                Ok(val) => amount = Some(val),
            }
        }
    }

    if let (Some(amount), Some(ty)) = (amount, maybe_ty) {

        let mut fail:Option<&'static str> = None;
        let ty_copy = ty.clone();
        pool.transaction(|txn| {
            let t = TransactionBuilder::new(
                amount,
                ty_copy.id,
                now,
            ).give(
                msg.author.id.into(),
                user.into(),
                false,
            ).message_id(msg.id);
            match TransferHandler::handle_single(txn, t) {
                Err(TransferError::NotEnough) => {
                    fail = Some("Insufficient balance.");
                    return Ok(());
                },
                Err(TransferError::Overflow) => {
                    fail = Some("Overflow.");
                    return Ok(());
                },
                Ok(v) => v?,
            }
            Ok(())
        }).await?;
        if let Some(fail_msg) = fail {
            msg.reply(&ctx, fail_msg).await?;
        }else{
            msg.reply(&ctx, format!(
                "Successfully transferred {} {} to {}.",
                amount,
                &ty.long_name_ambiguous,
                user.mention()
            )).await?;
        }
    } else if amount.is_none() {
        return Err("Amount not provided.".into());
    } else {
        return Err("Type not provided.".into());
    }
    
    Ok(())
}

#[allow(dead_code)] //Some strange bug means rust thinks this func isn't used, even when it definitely is.
async fn find_item_type(pool: &DbPool, ty_str:String) -> CommandResult<ItemType> {
    use diesel::prelude::*;
    use schema::item_types::dsl as it;
    use schema::item_type_aliases::dsl as ita;
    let maybe_res = ita::item_type_aliases
        .inner_join(it::item_types)
        .select(ItemType::cols())
        .filter(ita::alias.eq(&ty_str))
        .get_result_async(&*pool)
        .await
        .optional()?;
    match maybe_res {
        None => Err("Unrecognized type".into()),
        Some(v) => Ok(v),
    }
}

#[command]
async fn motion(ctx:&Context, msg:&Message, args:Args) -> CommandResult {
    motion_common(ctx, msg, args, BigDecimal::from(1)).await
}

#[command]
async fn supermotion(ctx:&Context, msg:&Message, args:Args) -> CommandResult {
    motion_common(ctx, msg, args, BigDecimal::from(2)).await
}

#[command]
async fn submotion(ctx:&Context, msg:&Message, args:Args) -> CommandResult {
    motion_common(ctx, msg, args, BigDecimal::from(0.5)).await
}

async fn motion_common(ctx:&Context, msg:&Message, args:Args, power: BigDecimal) -> CommandResult {
    trace!("motion_common");
    use diesel::prelude::*;
    use schema::motions::dsl as mdsl;
    use schema::motion_votes::dsl as mvdsl;
    use view_schema::balance_history::dsl as bhdsl;
    let motion_text = args.rest();
    //let mut motion_message_outer:Option<_> = None;
    let pool = Arc::clone(ctx.data.read().await.get::<DbPoolKey>().unwrap());

    let now = chrono::Utc::now();

    let motion_length_codepoints = motion_text.chars().count();
    if motion_length_codepoints > crate::MAX_MOTION_LENGTH_CODEPOINTS.into() {
        msg.reply(&ctx, format!(
            "Your motion is too long ({} codepoints out of max {})",
            motion_length_codepoints,
            crate::MAX_MOTION_LENGTH_CODEPOINTS,
        )).await?;
        return Ok(())
    }

    let balance:i64 = bhdsl::balance_history
        .select(bhdsl::balance)
        .filter(bhdsl::ty.eq("pc"))
        .filter(bhdsl::user.eq(msg.author.id.0 as i64))
        .order((bhdsl::happened_at.desc(), bhdsl::rowid.desc(), bhdsl::sign.desc()))
        .limit(1)
        .get_result_async(&*pool).await?;
    
    if balance < VOTE_BASE_COST as i64 {
        msg.reply(&ctx, "You don't have enough capital.").await?;
        return Ok(());
    }

    //According to motion#2960 "each member is limited to calling 10 motions per UTC day."
    let today_began_at = now.date().and_time(chrono::NaiveTime::from_hms(0,0,0)).unwrap();
    let motion_count = || mdsl::motions
        .filter(mdsl::motioned_by.eq(msg.author.id.0 as i64))
        .filter(mdsl::motioned_at.ge(today_began_at))
        .count();
    let motion_count_utc_today:i64 = motion_count().get_result_async(&*pool).await?;

    if motion_count_utc_today >= crate::MAX_MOTIONS_PER_DAY.into() {
        msg.reply(&ctx, "You have called too many motions today.").await?;
        return Ok(());
    }
    
    let motion_id:i64 = diesel::insert_into(schema::thing_ids::table).default_values().returning(schema::thing_ids::dsl::rowid).get_result_async(&*pool).await?;

    let cap_label = motion_label(&power);
    let mut bot_msg = serenity::model::id::ChannelId(MOTIONS_CHANNEL).send_message(&ctx, |m| {
        m.content(format!(
            "A motion has been called by {0}\nSay `$vote {1}` or visit {2}/motions/{1} to vote!",
            msg.author.mention(),
            damm::add_to_str(motion_id.to_string()),
            crate::SITE_URL,
        )).embed(|e| {
            e.field(cap_label, motion_text, false)
            .field("Votes", "**for 1**/0 against", false)
            .field(crate::names::name_of(msg.author.id), "1 for", true)
        })
    }).await?;

    let mut delete_message = false;

    pool.transaction(|txn| {
        diesel::sql_query("LOCK TABLE motions IN EXCLUSIVE MODE;").execute(&*txn)?;
        let motion_count_utc_today:i64 = motion_count().get_result(&*txn)?;
        let mut handle = TransferHandler::new(
            txn,
            vec![msg.author.id.into()],
            vec![CurrencyId::PC]
        )?;
        let balance = handle.balance(msg.author.id.into(), CurrencyId::PC);
        
        if balance < VOTE_BASE_COST.into() || motion_count_utc_today >= crate::MAX_MOTIONS_PER_DAY.into() {
            //msg.author is an asshat attempting to exploit race conditions or was part of an incredibly rare event
            delete_message = true;
            return Ok(());
        }

        let motion_id:i64 = diesel::insert_into(mdsl::motions).values((
            mdsl::rowid.eq(motion_id),
            mdsl::command_message_id.eq(msg.id.0 as i64),
            mdsl::bot_message_id.eq(bot_msg.id.0 as i64),
            mdsl::motion_text.eq(motion_text),
            mdsl::motioned_at.eq(now),
            mdsl::last_result_change.eq(now),
            mdsl::power.eq(power),
            mdsl::motioned_by.eq(msg.author.id.0 as i64),
        )).returning(mdsl::rowid).get_result(&*txn)?;

        diesel::insert_into(mvdsl::motion_votes).values((
            mvdsl::user.eq(msg.author.id.0 as i64),
            mvdsl::motion.eq(motion_id),
            mvdsl::direction.eq(true),
            mvdsl::amount.eq(1)
        )).execute(&*txn)?;

        let t = TransactionBuilder::new(
            VOTE_BASE_COST as i64,
            CurrencyId::PC,
            now,
        ).motion(msg.author.id.into(), motion_id, 1, true).message_id(msg.id);

        match handle.transfer(t) {
            Err(_) => unreachable!(),
            Ok(v) => v?,
        }

        Ok(())
    }).await?;
    if delete_message {
        // if this fails, what are you gonna do; send another message to a borked api? just ignore the error
        let _ = bot_msg.delete(&ctx).await;
    } else {
        update_motion_message(&ctx, Arc::clone(ctx.data.read().await.get::<DbPoolKey>().unwrap()), &mut bot_msg).await?;
        let mut emojis:Vec<_> = (*SPECIAL_EMOJI).iter().collect();
        emojis.sort_unstable_by_key(|(_,a)| match *a {
            SpecialEmojiAction::Direction(false) => -2,
            SpecialEmojiAction::Direction(true) => -1,
            SpecialEmojiAction::Amount(a) => (*a) as i64
        });
        for (emoji_id, _) in emojis {
            //dbg!(&emoji_id);
            serenity::model::id::ChannelId::from(MOTIONS_CHANNEL)
                .create_reaction(
                    &ctx,
                    &bot_msg,
                    serenity::model::channel::ReactionType::Custom{
                        animated: false,
                        id: (*emoji_id).into(),
                        name: Some("no".to_string())
                    }
                ).await?
            ;
        }
    }

    Ok(())
}

const YES_WORDS:&[&str] = &[
    "favor", 
    "for", 
    "approve", 
    "yes", 
    "y", 
    "aye", 
    "yeah", 
    "yeah!", 
    "\u{1ff4d}", 
    ":+1:", 
    ":thumbsup:",
    "\u{1f646}",
    ":ok_woman:",
    "\u{2b55}",
    ":o:",
    "\u{1f44c}",
    ":ok_hand:",
    "\u{1f197}",
    ":ok:",
    "\u{2705}",
    "pass",
];
const NO_WORDS :&[&str] = &[
    "neigh",
    "fail",
    "no", //no in sardinian
    "against",
    "no", //no in papiamento
    "nay",
    "no, asshole", //no in american english
    "no, you wanker", //no in british english
    "no, cunt", //no in australian english
    "no", //no in catalan
    "negative", 
    "no", //no in italian
    "never",
    "no", //no in friulan 
    "negatory", 
    "no", //no in spanish 
    "veto", 
    "no", //no in ligurian
    "\u{1f44e}", 
    "deny",
    ":-1:", 
    ":thumbsdown:",
    ".i na go'i", //no in lojban
    "\u{1f645}",
    ":no_good:",
    "\u{274C}",
    "\u{1f196}",
    ":ng:",
    "naw",
];
const ZERO_WORDS:&[&str] = &["zero", "zerovote", "nil", "nada", "nothing"];
const IGNORE_WORDS:&[&str] = &["in", "i", "I", "think", "say", "fuck", "hell"];

#[command]
#[min_args(1)]
async fn vote(ctx:&Context, msg:&Message, mut args:Args) -> CommandResult {
    trace!("vote");
    let checksummed_motion_id:String = args.single()?;
    //dbg!(&checksummed_motion_id);
    if let Some(digit_arr) = damm::validate(&checksummed_motion_id) {
        let mut motion_id:i64 = 0;
        for d in &digit_arr {
            motion_id *= 10;
            motion_id += *d as i64;
        }
        let motion_id = motion_id;
        //dbg!(&motion_id);

        let mut vote_count = 1;
        let mut vote_direction:Option<bool> = None;
        for args_result in args.iter::<String>() {
            //dbg!(&args_result);
            let arg = args_result?;
            if YES_WORDS.contains(&&*arg) {
                vote_direction = Some(true);
            }else if NO_WORDS.contains(&&*arg) {
                vote_direction = Some(false);
            }else if ZERO_WORDS.contains(&&*arg) {
                vote_count = 0;
            }else if IGNORE_WORDS.contains(&&*arg) {
                //ignore
            }else {
                match arg.parse():Result<u32, _> {
                    Err(e) => return Err(e.into()),
                    Ok(v) => vote_count = v as i64,
                }
            }
        }

        let pool = Arc::clone(ctx.data.read().await.get::<DbPoolKey>().unwrap());
        let resp = vote_common_async(
            pool,
            vote_direction,
            vote_count,
            msg.author.id.into(),
            Some(motion_id),
            None,
            Some(msg.id.0 as i64),
        ).await;

        msg.reply(ctx, resp).await.unwrap();
    }else{
        return Err("Invalid motion id, please try again.".into());
    }
    Ok(())
}

use std::borrow::Cow;

pub async fn vote_common_async(
    pool: Arc<DbPool>,
    vote_direction:Option<bool>,
    vote_count:i64,
    user_id:models::UserId,
    motion_id:Option<i64>,
    message_id:Option<i64>,
    command_message_id:Option<i64>,
) -> Cow<'static, str> {
    trace!("vote_common_async");
    task::spawn_blocking(move || {
        let conn = pool.get().unwrap();
        vote_common(
            &*conn,
            vote_direction,
            vote_count,
            user_id,
            motion_id,
            message_id,
            command_message_id,
        )
    }).await.unwrap()
}

// This function is called from synchronous rocket code, so must remain sync.
pub fn vote_common(
    //ctx: &Context,
    conn: &diesel::PgConnection,
    vote_direction:Option<bool>,
    vote_count:i64,
    user_id:models::UserId,
    motion_id:Option<i64>,
    message_id:Option<i64>,
    command_message_id:Option<i64>,
) -> Cow<'static, str> {
    trace!("vote_common");
    let now = chrono::Utc::now();
    if vote_count < 0 {
        return Cow::Borrowed("Can not vote a negative number of times.");
    }
    let mut fail:Option<&'static str> = None;
    let mut outer_cost:Option<i64> = None;
    let mut outer_motion_id:Option<i64> = None;
    let mut outer_vote_ordinal_start:Option<i64> = None;
    let mut outer_vote_ordinal_end:Option<i64> = None;
    let mut outer_direction:Option<bool> = None;
    let txn_res = conn.transaction::<_, diesel::result::Error, _>(|| {
        use diesel::prelude::*;
        use crate::schema::motions::dsl as mdsl;
        use crate::schema::motion_votes::dsl as mvdsl;

        let res:Option<(i64, bool, BigDecimal, i64)> = mdsl::motions
        .filter(mdsl::rowid.eq(motion_id.unwrap_or(-1)).or(mdsl::bot_message_id.eq(message_id.unwrap_or(-1))))
        .select((
            mdsl::rowid,
            mdsl::announcement_message_id.is_null().and(
                mdsl::last_result_change.gt(chrono::Utc::now() - *crate::MOTION_EXPIRATION)
            ),
            mdsl::power,
            mdsl::bot_message_id,
        ))
        .for_update()
        .get_result(conn)
        .optional()?;
        //dbg!(&res);

        let mut handle = TransferHandler::new(
            conn,
            vec![user_id],
            vec![CurrencyId::PC],
        )?;

        if let Some((motion_id, not_announced, power, _motion_message_id)) = res {
            outer_motion_id = Some(motion_id);
            if not_announced {
                //dbg!();
                mvdsl::motion_votes //obtain a lock on all votes
                .select(mvdsl::amount)
                .filter(mvdsl::motion.eq(motion_id))
                .for_update()
                .execute(&*conn)?;

                //dbg!();
                let voted_so_far:i64;
                let outer_dir:bool;
                let maybe_vote_res:Option<(bool, i64)> = mvdsl::motion_votes
                .filter(mvdsl::motion.eq(motion_id))
                .filter(mvdsl::user.eq(user_id))
                .select((mvdsl::direction, mvdsl::amount))
                .for_update()
                .get_result(&*conn)
                .optional()?;
                //dbg!();

                if let Some((dir, count)) = maybe_vote_res {
                    if let Some(requested_dir) = vote_direction {
                        if requested_dir != dir {
                            fail = Some("You cannot change your vote.");
                            return Err(diesel::result::Error::RollbackTransaction);
                        }
                    }
                    voted_so_far = count;
                    outer_dir = dir;
                } else {
                    if vote_direction.is_none() {
                        fail = Some("You must specify how you want to vote!");
                        return Err(diesel::result::Error::RollbackTransaction);
                    }
                    //dbg!();
                    diesel::insert_into(mvdsl::motion_votes).values((
                        mvdsl::motion.eq(motion_id),
                        mvdsl::user.eq(user_id),
                        mvdsl::amount.eq(0),
                        mvdsl::direction.eq(vote_direction.unwrap()),
                    )).on_conflict_do_nothing().execute(&*conn)?;
                    //dbg!();

                    let vote_res:(bool, i64) = mvdsl::motion_votes
                    .filter(mvdsl::motion.eq(motion_id))
                    .filter(mvdsl::user.eq(user_id))
                    .select((mvdsl::direction, mvdsl::amount))
                    .for_update()
                    .get_result(&*conn)?;
                    //dbg!(&vote_res);
                    voted_so_far = vote_res.1;
                    outer_dir = vote_res.0;
                }
                outer_direction = Some(outer_dir);

                let ordinal_start = voted_so_far + 1;
                let ordinal_end = match (voted_so_far + 1).checked_add(vote_count) {
                    Some(v) => v,
                    None => {
                        fail = Some("Overflow");
                        return Err(diesel::result::Error::RollbackTransaction);
                    }
                };

                //dbg!(&voted_so_far, &outer_dir, &vote_count);
                let mut cost:i64 = 0;
                outer_vote_ordinal_start = Some(ordinal_start);
                outer_vote_ordinal_end = Some(ordinal_end);
                let mut do_fail = false;
                for nth in ordinal_start..ordinal_end {
                    //effectively:
                    //cost += nth_vote_cost(nth).unwrap();
                    if let Ok(this_vote_cost) = nth_vote_cost(nth) {
                        if let Some(new_total_cost) = cost.checked_add(this_vote_cost) {
                            cost = new_total_cost
                        } else {
                            do_fail = true;
                            break;
                        }
                    } else {
                        do_fail = true;
                        break;
                    }
                }
                if do_fail {
                    fail = Some("Integer overflow, no way you have that much pc");
                    return Err(diesel::result::Error::RollbackTransaction);
                }
                //dbg!(&cost);
                outer_cost = Some(cost);

                let t = TransactionBuilder::new(
                    cost,
                    CurrencyId::PC,
                    now,
                ).motion(
                    user_id,
                    motion_id,
                    vote_count,
                    false,
                );
                let t = if let Some(message_id) = command_message_id {
                    t.message_id_raw(message_id)
                } else { t };
                match handle.transfer(t) {
                    Err(TransferError::Overflow) => {
                        fail = Some("Integer overflow, no way you have that much pc");
                        return Err(diesel::result::Error::RollbackTransaction);
                    },
                    Err(TransferError::NotEnough) => {
                        fail = Some("Not enough capital.");
                        return Err(diesel::result::Error::RollbackTransaction);
                    },
                    Ok(v) => v?,
                }

                use bigdecimal::ToPrimitive;
                let get_vote_count = |dir:bool| -> Result<i64, diesel::result::Error> {
                    let votes:Option<BigDecimal> = mvdsl::motion_votes
                    .select(diesel::dsl::sum(mvdsl::amount))
                    .filter(mvdsl::motion.eq(motion_id))
                    .filter(mvdsl::direction.eq(dir))
                    .get_result(&*conn)?;
                    Ok(votes.map(|bd| bd.to_i64().unwrap()).unwrap_or(0))
                };
                let mut yes_votes = get_vote_count(true)?;
                let mut no_votes = get_vote_count(false)?;
                //dbg!(&yes_votes, &no_votes);
                

                let result_before = is_win(yes_votes, no_votes, &power);
                if outer_dir {
                    yes_votes += vote_count;
                }else{
                    no_votes += vote_count;
                }
                let result_after = is_win(yes_votes, no_votes, &power);

                diesel::update(
                    mvdsl::motion_votes.filter(mvdsl::motion.eq(motion_id)).filter(mvdsl::user.eq(user_id))
                ).set(
                    mvdsl::amount.eq(voted_so_far + vote_count)
                ).execute(&*conn)?;
                //dbg!();

                if result_before != result_after {
                    diesel::update(mdsl::motions.filter(mdsl::rowid.eq(motion_id))).set(
                        mdsl::last_result_change.eq(chrono::Utc::now())
                    ).execute(&*conn)?;
                    //dbg!();
                }
                //dbg!();

                // let mut motion_message = ctx.http.get_message(MOTIONS_CHANNEL, motion_message_id as u64).unwrap();
                // update_motion_message(ctx, &*conn, &mut motion_message).unwrap(); 
                diesel::update(mdsl::motions.filter(mdsl::rowid.eq(motion_id))).set(mdsl::needs_update.eq(true)).execute(&*conn).unwrap();
            }else{
                fail = Some("Motion has expired.");
                return Err(diesel::result::Error::RollbackTransaction);
            }
        }else{
            fail = Some("Motion not found.");
            return Err(diesel::result::Error::RollbackTransaction);
        }

        Ok(())
    });
    if let Some(msg) = fail {
        return Cow::Borrowed(msg);
    }
    txn_res.unwrap();
    if let (Some(cost), Some(motion_id), Some(ordinal_start), Some(ordinal_end), Some(direction)) = (outer_cost, outer_motion_id, outer_vote_ordinal_start, outer_vote_ordinal_end, outer_direction) {
        #[allow(clippy::comparison_chain)]
        let ordinal_text = if vote_count > 1 {
            format!(", {} to {} vote", ordinal::Ordinal(ordinal_start), ordinal::Ordinal(ordinal_end-1))
        } else if vote_count == 1 {
            format!(", {} vote", ordinal::Ordinal(ordinal_start))
        } else { String::new() };
        return Cow::Owned(format!(
            "Voted {} times {} motion #{}{}, costing {} capital",
            vote_count,
            if direction { "for" } else { "against" },
            damm::add_to_str(motion_id.to_string()),
            ordinal_text,
            cost,
        ));
    }
    Cow::Borrowed("Vote cast")
}
