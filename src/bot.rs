use crate::schema;
use crate::view_schema;
use crate::damm;

use std::sync::Arc;
use std::time::Duration;

use tokio_diesel::*;

use serenity::client::Client;
use serenity::model::misc::Mentionable;
use serenity::model::channel::Message;
use serenity::model::id::UserId;
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

use crate::is_win::is_win;
use crate::tasks;

pub type DbPool = diesel::r2d2::Pool<diesel::r2d2::ConnectionManager<diesel::PgConnection>>;

struct DbPoolKey;
impl serenity::prelude::TypeMapKey for DbPoolKey {
    type Value = Arc<DbPool>;
}

#[group]
#[commands(ping, give, force_give, balances, motion, supermotion, vote, hack_message_update, help, version_info)]
struct General;

#[group]
#[commands(fabricate)]
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

#[cfg(feature = "debug")]
lazy_static! {
    pub static ref GENERATE_EVERY:chrono::Duration = chrono::Duration::seconds(30);
    pub static ref MOTION_EXPIRATION:chrono::Duration = chrono::Duration::minutes(20);
}

#[cfg(not(feature = "debug"))]
lazy_static! {
    pub static ref GENERATE_EVERY:chrono::Duration = chrono::Duration::hours(24);
    pub static ref MOTION_EXPIRATION:chrono::Duration = chrono::Duration::hours(48);
}

pub const VOTE_BASE_COST:u16 = 40;
#[cfg(not(feature = "debug"))]
pub const MOTIONS_CHANNEL:u64 = 609093491150028800; //bureaucracy channel
#[cfg(feature = "debug")]
//const MOTIONS_CHANNEL:u64 = 694013828362534983; //pluto-dev channel
//const MOTIONS_CHANNEL:u64 = 610387757818183690; //test channel in shelvacuisawesomeserver
//const MOTIONS_CHANNEL:u64 = 560918427091468387; //spam channel
pub const MOTIONS_CHANNEL:u64 = 770726979456466954; //pluto-beta-messages in CONceptualization

#[cfg(not(feature = "debug"))]
pub const MY_ID_INT:u64 = 690112509537288202;
#[cfg(feature = "debug")]
pub const MY_ID_INT:u64 = 698996983305863178;

pub const MY_ID:UserId = UserId(MY_ID_INT);

#[async_trait]
trait FromCommandArgs : Sized {
    async fn from_command_args(ctx: &Context, msg: &Message, arg: &str) -> Result<Self, &'static str>;
}

#[async_trait]
impl FromCommandArgs for UserId {
    async fn from_command_args(ctx: &Context, msg: &Message, arg: &str) -> Result<Self, &'static str> {
        trace!("from_command_args");
        if arg == "." || arg == "self" {
            return Ok(msg.author.id);
        }
        if let Ok(raw_id) = arg.parse():Result<u64,_> {
            return Ok(UserId::from(raw_id));
        }

        if let Some(ma) = USER_PING_RE.captures(arg) {
            if let Ok(raw_id) = ma.get(1).unwrap().as_str().parse():Result<u64,_> {
                return Ok(UserId::from(raw_id));
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
                        user_id.0 as i64,
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

pub fn name_of(u:UserId) -> Cow<'static, str> {
    trace!("name_of");
    match u.0 {
        165858230327574528 => "Shelvacu".into(),
        125003180219170816 => "Colin".into(),
        155438323354042368 => "Ben".into(),
        n => n.to_string().into(),
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
async fn after_hook(_: &Context, _: &Message, cmd_name: &str, error: Result<(), CommandError>) {
    trace!("after_hook");
    //  Print out an error if it happened
    if let Err(why) = error {
        println!("Error in {}: {:?}", cmd_name, why);
    }
}

pub async fn bot_main() {
    trace!("bot_main begin");
    lazy_static::initialize(&GENERATE_EVERY);
    lazy_static::initialize(&USER_PING_RE);
    lazy_static::initialize(&MOTION_EXPIRATION);

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
    let mut client = Client::builder(&env::var("DISCORD_TOKEN").expect("token"))
        .event_handler(Handler)
        .framework(framework)
        .await
        .expect("Error creating client");
    trace!("Client configured");
    let mut write_handle = client.data.write().await;
    write_handle.insert::<DbPoolKey>(Arc::clone(&arc_pool));
    drop(write_handle);
    let cnh = Arc::clone(&client.cache_and_http);
    let announce_threads_pool = Arc::clone(&arc_pool);
    let announce_threads_http = Arc::clone(&cnh);
    task::spawn(async move {
        loop {
            std::thread::sleep(Duration::from_millis(500));           
            match tasks::process_motion_completions(Arc::clone(&announce_threads_pool), &announce_threads_http).await {
                Ok(()) => (),
                Err(e) => warn!("Could not query for completed motions, {:?}", e),
            }
        }
    });
    trace!("Made announce thread");

    let threads_pool = Arc::clone(&arc_pool);
    task::spawn(async move {
        loop {
            /* not properly locking, but should only have one thread trying to access */
            std::thread::sleep(Duration::from_millis(500));
            match tasks::process_generators(Arc::clone(&threads_pool)).await {
                Ok(()) => (),
                Err(e) => warn!("Could process generator payouts, {:?}", e),
            }
        }
    });
    drop(arc_pool);
    trace!("Made process_generators thread");

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
    
    let (motion_text, motion_id, is_super) = mdsl::motions.filter(mdsl::bot_message_id.eq(msg.id.0 as i64)).select((mdsl::motion_text, mdsl::rowid, mdsl::is_super)).get_result_async(&*pool).await?:(String, i64, bool);
    #[derive(Queryable,Debug)]
    struct MotionVote {
        user:i64,
        amount:i64,
        direction:bool,
    }
    let mut votes:Vec<MotionVote> = mvdsl::motion_votes.filter(mvdsl::motion.eq(motion_id)).select((mvdsl::user, mvdsl::amount, mvdsl::direction)).get_results_async(&*pool).await?;
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
    let pass = is_win(yes_votes, no_votes, is_super);
    let cap_label = if is_super { "Supermotion" } else { "Simple Motion" };
    msg.edit(cnh, |m| {
        m.embed(|e| {
            e.field(cap_label, motion_text, false);
            if pass {
                e.field("Votes", format!("**for {}**/{} against", yes_votes, no_votes), false);
            } else {
                e.field("Votes", format!("**against {}**/{} for", no_votes, yes_votes), false);
            }
            for vote in &votes[0..std::cmp::min(votes.len(),21)] {
                e.field(name_of(UserId::from(vote.user as u64)), format!("{} {}", vote.amount, if vote.direction {"for"} else {"against"}), true);
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

#[command]
#[num_args(2)]
async fn fabricate(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    trace!("fabricate");
    use diesel::prelude::*;
    use schema::item_types::dsl as it;
    use schema::item_type_aliases::dsl as ita;
    let pool = Arc::clone(ctx.data.read().await.get::<DbPoolKey>().unwrap());

    let ty:ItemType;
    let ty_str:String = args.single()?;
    let alias:Option<ItemType> = ita::item_type_aliases
        .inner_join(it::item_types)
        .select(it::item_types::all_columns())
        .filter(ita::alias.eq(&ty_str))
        .get_result_async(&*pool)
        .await
        .optional()?;
    if let Some(it) = alias {
        ty = it;
    } else {
        return Err("Unrecognized type".into());
    }
    let how_many:i64 = args.single()?;
    if how_many <= 0 {
        return Err("fuck".into());
    }
    let user:UserId;
    if args.remaining() > 0 {
        let user_str = args.single()?:String;
        user = UserId::from_command_args(ctx, msg, &user_str).await?;
    }else{
        user = msg.author.id;
    }

    pool.transaction(|txn| {
        use view_schema::balance_history::dsl as bh;
        use schema::transfers::dsl as tdsl;
        let prev_balance:i64 = view_schema::balance_history::table
          .select(bh::balance)
          .filter(bh::user.eq(user.0 as i64))
          .filter(bh::ty.eq(ty.db_name()))
          .order(bh::happened_at.desc())
          .limit(1)
          .for_update()
          .get_result(&*txn)
          .optional()?
          .unwrap_or(0);
        
        diesel::insert_into(tdsl::transfers).values((
            tdsl::quantity.eq(how_many),
            tdsl::to_user.eq(msg.author.id.0 as i64),
            tdsl::to_balance.eq(prev_balance + how_many),
            tdsl::happened_at.eq(chrono::Utc::now()),
            tdsl::message_id.eq(msg.id.0 as i64),
            tdsl::ty.eq(ty.db_name()),
            tdsl::transfer_ty.eq("command_fabricate"),
        )).execute(&*txn)?;

        Ok(())
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
        env!("VERGEN_GIT_SEMVER"),
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
        .get_results_async(&*pool).await?;
    
    let mut balances = Vec::new();
    for it in item_types {
        let bal = bh::balance_history
        .select(bh::balance)
        .filter(bh::user.eq(msg.author.id.0 as i64))
        .filter(bh::ty.eq(it.db_name()))
        .order(bh::happened_at.desc())
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

#[derive(Debug, PartialEq, Eq, Clone, Queryable)]
struct ItemType{
    pub name: String,
    pub long_name_plural: String,
    pub long_name_ambiguous: String,
}

impl ItemType {
    pub fn db_name(&self) -> &str {
        self.name.as_str()
    }
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
    let pool = Arc::clone(ctx.data.read().await.get::<DbPoolKey>().unwrap());

    let user_str:String = args.single()?;
    let user = UserId::from_command_args( ctx, msg, &user_str ).await?;
    if check_user && !ctx.cache.users().await.contains_key(&user) {
        return Err("User not found".into());
    }
    let mut maybe_ty:Option<ItemType> = None;
    let mut amount:Option<u64> = None;
    for arg_result in args.iter::<String>(){
        let arg = arg_result.unwrap();
        let alias:Option<ItemType> = ita::item_type_aliases
            .inner_join(it::item_types)
            .select(it::item_types::all_columns())
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
                    .select(it::item_types::all_columns())
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

            match count_str.parse():Result<u64,_> {
                Err(e) => return Err(format!("Bad count {:?}", e).into()),
                Ok(val) => amount = Some(val),
            }
        }else{
            match arg.parse():Result<u64, _> {
                Err(e) => return Err(format!("Bad count {:?}", e).into()),
                Ok(val) => amount = Some(val),
            }
        }
    }

    if let (Some(amount), Some(ty)) = (amount, maybe_ty) {

        let mut fail:Option<&'static str> = None;
        let ty_copy = ty.clone();
        pool.transaction(|txn| {

            use view_schema::balance_history::dsl as bh;
            let mut ids = [msg.author.id.0, user.0];
            let mut author = 0;
            let mut dest = 1;
            if ids[0] > ids[1] {
                ids = [ids[1],ids[0]];
                author = 1;
                dest = 0;
            }
            let balances:Vec<i64> = ids.iter().map::<Result<i64,diesel::result::Error>,_>(|id| {
                Ok(
                    bh::balance_history
                        .select(bh::balance)
                        .filter(bh::user.eq(*id as i64))
                        .filter(bh::ty.eq(ty_copy.db_name()))
                        .order(bh::happened_at.desc())
                        .limit(1)
                        .for_update()
                        .get_result(&*txn)
                        .optional()?
                        .unwrap_or(0i64)
                )
            }).collect::<Result<_,_>>()?;
            let sender_balance = balances[author];
            let dest_balance = balances[dest];
            if sender_balance < amount as i64 {
                fail = Some("Insufficient balance.");
                return Ok(());
            }

            use schema::transfers;
            #[derive(Insertable, Debug)]
            #[table_name = "transfers"]
            struct Transfer {
                from_user:i64,
                quantity:i64,
                to_user:i64,
                from_balance:i64,
                to_balance:i64,
                happened_at:chrono::DateTime<chrono::Utc>,
                message_id:i64,
                ty:String,
                transfer_ty:&'static str,
            }

            let from_balance;
            let to_balance;
            if msg.author.id == user {
                from_balance = sender_balance;
                to_balance = sender_balance;
            }else{
                from_balance = sender_balance - amount as i64;
                to_balance = dest_balance + amount as i64;
            }

            let t = Transfer {
                from_user: msg.author.id.0 as i64,
                quantity: amount as i64,
                to_user: user.0 as i64,
                from_balance,
                to_balance,
                happened_at: chrono::Utc::now(),
                message_id: msg.id.0 as i64,
                ty: ty.db_name().into(),
                transfer_ty: "give",
            };

            diesel::insert_into(schema::transfers::table).values(&t).execute(&*txn)?;

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

#[command]
async fn motion(ctx:&Context, msg:&Message, args:Args) -> CommandResult {
    motion_common(ctx, msg, args, false).await
}

#[command]
async fn supermotion(ctx:&Context, msg:&Message, args:Args) -> CommandResult {
    motion_common(ctx, msg, args, true).await
}

async fn motion_common(ctx:&Context, msg:&Message, args:Args, is_super: bool) -> CommandResult {
    trace!("motion_common");
    use diesel::prelude::*;
    use schema::motions::dsl as mdsl;
    use schema::motion_votes::dsl as mvdsl;
    use schema::transfers::dsl as tdsl;
    use view_schema::balance_history::dsl as bhdsl;
    let motion_text = args.rest();
    //let mut motion_message_outer:Option<_> = None;
    let pool = Arc::clone(ctx.data.read().await.get::<DbPoolKey>().unwrap());

    let now = chrono::Utc::now();

    let balance:i64 = bhdsl::balance_history
        .select(bhdsl::balance)
        .filter(bhdsl::ty.eq("pc"))
        .filter(bhdsl::user.eq(msg.author.id.0 as i64))
        .order(bhdsl::happened_at.desc())
        .limit(1)
        .get_result_async(&*pool).await?;
    
    if balance < VOTE_BASE_COST as i64 {
        msg.reply(&ctx, "You don't have enough capital.").await?;
        return Ok(());
    }
    
    let motion_id:i64 = diesel::insert_into(schema::motion_ids::table).default_values().returning(schema::motion_ids::dsl::rowid).get_result_async(&*pool).await?;

    let cap_label = if is_super { "Supermotion" } else { "Simple Motion" };
    let mut bot_msg = serenity::model::id::ChannelId(MOTIONS_CHANNEL).send_message(&ctx, |m| {
        m.content(format!(
            "A motion has been called by {}\n`$vote {}` to vote!",
            msg.author.mention(),
            damm::add_to_str(motion_id.to_string())
        )).embed(|e| {
            e.field(cap_label, motion_text, false)
            .field("Votes", "**for 1**/0 against", false)
            .field(name_of(msg.author.id), "1 for", true)
        })
    }).await?;

    pool.transaction(|txn| {
        let balance:i64 = bhdsl::balance_history
            .select(bhdsl::balance)
            .filter(bhdsl::ty.eq("pc"))
            .filter(bhdsl::user.eq(msg.author.id.0 as i64))
            .order(bhdsl::happened_at.desc())
            .limit(1)
            .for_update()
            .get_result(&*txn)?;
        
        if balance < VOTE_BASE_COST as i64 {
            //msg.author is an asshat or was part of an incredibly rare event
            return Ok(());
        }

        let motion_id:i64 = diesel::insert_into(mdsl::motions).values((
            mdsl::rowid.eq(motion_id),
            mdsl::command_message_id.eq(msg.id.0 as i64),
            mdsl::bot_message_id.eq(bot_msg.id.0 as i64),
            mdsl::motion_text.eq(motion_text),
            mdsl::motioned_at.eq(now),
            mdsl::last_result_change.eq(now),
            mdsl::is_super.eq(is_super),
            mdsl::motioned_by.eq(msg.author.id.0 as i64),
        )).returning(mdsl::rowid).get_result(&*txn)?;

        diesel::insert_into(mvdsl::motion_votes).values((
            mvdsl::user.eq(msg.author.id.0 as i64),
            mvdsl::motion.eq(motion_id),
            mvdsl::direction.eq(true),
            mvdsl::amount.eq(1)
        )).execute(&*txn)?;

        diesel::insert_into(tdsl::transfers).values((
            tdsl::from_user.eq(msg.author.id.0 as i64),
            tdsl::from_balance.eq(balance),
            tdsl::ty.eq("pc"),
            tdsl::quantity.eq(VOTE_BASE_COST as i64),
            tdsl::happened_at.eq(chrono::Utc::now()),
            tdsl::message_id.eq(msg.id.0 as i64),
            tdsl::to_motion.eq(motion_id),
            tdsl::to_votes.eq(1),
            tdsl::transfer_ty.eq("motion_create"),
        )).execute(&*txn)?;

        Ok(())
    }).await?;
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
            msg.author.id.0 as i64,
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
    user_id:i64,
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
    user_id:i64,
    motion_id:Option<i64>,
    message_id:Option<i64>,
    command_message_id:Option<i64>,
) -> Cow<'static, str> {
    trace!("vote_common");
    let mut fail:Option<&'static str> = None;
    let mut outer_cost:Option<i64> = None;
    let mut outer_motion_id:Option<i64> = None;
    let mut outer_vote_ordinal_start:Option<i64> = None;
    let mut outer_vote_ordinal_end:Option<i64> = None;
    let mut outer_direction:Option<bool> = None;
    let txn_res = conn.transaction::<_, diesel::result::Error, _>(|| {
        use diesel::prelude::*;
        use schema::motions::dsl as mdsl;
        use schema::motion_votes::dsl as mvdsl;
        use view_schema::balance_history::dsl as bhdsl;
        use schema::transfers::dsl as tdsl;

        let res:Option<(i64, bool, bool, i64)> = mdsl::motions
        .filter(mdsl::rowid.eq(motion_id.unwrap_or(-1)).or(mdsl::bot_message_id.eq(message_id.unwrap_or(-1))))
        .select((mdsl::rowid, mdsl::announcement_message_id.is_null(), mdsl::is_super, mdsl::bot_message_id))
        .for_update()
        .get_result(conn)
        .optional()?;
        //dbg!(&res);

        if let Some((motion_id, not_announced, is_super, _motion_message_id)) = res {
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

                //dbg!(&voted_so_far, &outer_dir, &vote_count);
                let mut cost:i64 = 0;
                outer_vote_ordinal_start = Some(voted_so_far + 1);
                outer_vote_ordinal_end = Some(voted_so_far + vote_count + 1);
                let mut do_fail = false;
                for nth in voted_so_far+1..voted_so_far+vote_count+1 {
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

                let balance:i64 = bhdsl::balance_history
                .select(bhdsl::balance)
                .filter(bhdsl::user.eq(user_id))
                .filter(bhdsl::ty.eq("pc"))
                .order(bhdsl::happened_at.desc())
                .limit(1)
                .for_update()
                .get_result(&*conn)
                .optional()?
                .unwrap_or(0);
                //dbg!(&balance);

                if cost > balance {
                    fail = Some("Not enough capital.");
                    return Err(diesel::result::Error::RollbackTransaction);
                }

                let now = chrono::Utc::now();

                diesel::insert_into(tdsl::transfers).values((
                    tdsl::ty.eq("pc"),
                    tdsl::from_user.eq(user_id),
                    tdsl::quantity.eq(cost),
                    tdsl::from_balance.eq(balance - cost),
                    tdsl::happened_at.eq(now),
                    tdsl::message_id.eq(command_message_id),
                    tdsl::to_motion.eq(motion_id),
                    tdsl::to_votes.eq(vote_count),
                    tdsl::transfer_ty.eq("motion_vote"),
                )).execute(&*conn)?;
                //dbg!();

                use bigdecimal::{BigDecimal,ToPrimitive};
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
                
                let result_before:bool;
                let result_after:bool;
                result_before = is_win(yes_votes, no_votes, is_super);
                if outer_dir {
                    yes_votes += vote_count;
                }else{
                    no_votes += vote_count;
                }
                result_after = is_win(yes_votes, no_votes, is_super);

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
