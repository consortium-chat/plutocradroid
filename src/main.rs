#![feature(type_ascription)]
#[macro_use]
extern crate diesel;
#[macro_use]
extern crate lazy_static;

mod schema;
mod view_schema;
use std::sync::Arc;
use std::thread;
use serenity::client::Client;
use serenity::model::channel::Message;
use serenity::model::id::UserId;
use serenity::prelude::{EventHandler, Context};
use serenity::framework::standard::{
    StandardFramework,
    CommandResult,
    macros::{
        command,
        group
    },
    Args,
};
use regex::Regex;

use diesel::connection::Connection;

struct DbPoolKey;

impl serenity::prelude::TypeMapKey for DbPoolKey {
    type Value = Arc<diesel::r2d2::Pool<diesel::r2d2::ConnectionManager<diesel::PgConnection>>>;
}

#[group]
#[commands(ping, fabricate_gens, fabricate_pc, give, balances)]
struct General;

use std::env;

struct Handler;

impl EventHandler for Handler {}

lazy_static! {
    static ref USER_PING_RE: Regex = Regex::new(r"^\s*<@!?(\d+)>\s*$").unwrap();
}

const GENERATE_EVERY_S:u32 = 30/*86400*/;

trait FromCommandArgs : Sized {
    fn from_command_args(ctx: &Context, msg: &Message, arg: &str) -> Result<Self, &'static str>;
}

impl FromCommandArgs for UserId {
    fn from_command_args(ctx: &Context, msg: &Message, arg: &str) -> Result<Self, &'static str> {
        if arg == "." || arg == "self" {
            return Ok(msg.author.id);
        }
        // if arg == "last" || arg == "him" || arg == "her" || arg == "them" {
        //     //TODO: find message before the current one that isn't from author in the same channel, and return that UserId
        // }
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
                    let cache = ctx.cache.read();
                    let maybe_user = cache
                        .users
                        .values()
                        .find(|user_lock| {
                            let user = user_lock.read();
                            user.discriminator == discriminator && user.name.to_ascii_uppercase() == name.to_ascii_uppercase()
                        });
                    if let Some(user_lock) = maybe_user {
                        return Ok(user_lock.read().id);
                    }
                }
            }
        }

        for (_id, guild_lock) in &ctx.cache.read().guilds {
            let guild = guild_lock.read();
            for member in guild.members.values() {
                if let Some(nick) = member.nick.as_ref() {
                    if nick.to_ascii_uppercase() == arg.to_ascii_uppercase() {
                        return Ok(member.user.read().id);
                    }
                }
                let user = member.user.read();
                if user.name.to_ascii_uppercase() == arg.to_ascii_uppercase() {
                    return Ok(user.id);
                }
            }
        }
        Err("Could not find any User.")
    }
}

fn main() {
    dotenv::dotenv().unwrap();

    let pool = diesel::r2d2::Builder::new().build(diesel::r2d2::ConnectionManager::<diesel::PgConnection>::new(&env::var("DATABASE_URL").expect("DATABASE_URL expected"))).expect("could not build DB pool");
    let arc_pool = Arc::new(pool);
    // Login with a bot token from the environment
    let mut client = Client::new(&env::var("DISCORD_TOKEN").expect("token"), Handler)
        .expect("Error creating client");
    let mut write_handle = client.data.write();
    write_handle.insert::<DbPoolKey>(Arc::clone(&arc_pool));
    drop(write_handle);
    client.with_framework(StandardFramework::new()
        .configure(|c| c.prefix("$")) // set the bot's prefix to "~"
        .group(&GENERAL_GROUP)
        .on_dispatch_error(|_ctx, msg, err| {
            println!(
                "{:?}\nerr'd with {:?}",
                msg, err
            );
        })
        .after(|ctx, msg, _command_name, res| {
            if let Err(e) = res {
                msg.reply(ctx, format!("ERR: {:?}", e)).unwrap();
            }
            // println!(
            //     "{:#?}\n{:?} {:?}",
            //     msg, s, res
            // );
        })
    );

    let threads_conn = arc_pool.get().unwrap();
    thread::spawn(move || {
        use schema::gen::dsl as gdsl;
        use diesel::prelude::*;
        let conn = threads_conn;

        loop {
            let now = chrono::Utc::now();
            let then = now - chrono::Duration::seconds(GENERATE_EVERY_S as i64);
            let mut was_empty = true;
            conn.transaction::<_, diesel::result::Error, _>(|| {
                let to_payout:Vec<(i64, Option<i64>, chrono::DateTime<chrono::Utc>,)> = gdsl::gen
                    .select((gdsl::rowid, gdsl::owner, gdsl::last_payout))
                    .filter(gdsl::last_payout.lt(then))
                    .limit(2000)
                    .for_update()
                    .get_results(&*conn)?;

                diesel::sql_query("LOCK TABLE pc_transfers IN EXCLUSIVE MODE;").execute(&*conn)?;
                
                let mut balances = <std::collections::HashMap<i64,i64>>::new();
                for (rowid, maybe_owner, last_payout) in &to_payout {
                    if let Some(owner) = maybe_owner {
                        use schema::pc_transfers::dsl as pcdsl;
                        let balance = balances.entry(*owner).or_insert_with(|| {
                            use view_schema::balance_history::dsl as bhdsl;
                            bhdsl::balance_history
                            .select(bhdsl::balance)
                            .filter(bhdsl::user.eq(owner))
                            .order(bhdsl::happened_at.desc())
                            .limit(1)
                            .get_result(&*conn)
                            .optional()
                            .unwrap()
                            .unwrap_or(0)
                        });

                        *balance += 1;

                        diesel::insert_into(pcdsl::pc_transfers).values((
                            pcdsl::from_gen.eq(rowid),
                            pcdsl::quantity.eq(1),
                            pcdsl::to_user.eq(owner),
                            pcdsl::to_balance.eq(*balance),
                            pcdsl::happened_at.eq(&now),
                        )).execute(&*conn).unwrap();
                    }

                    diesel::update(gdsl::gen.filter(gdsl::rowid.eq(rowid)))
                    .set(gdsl::last_payout.eq(*last_payout + chrono::Duration::seconds(GENERATE_EVERY_S as i64)))
                    .execute(&*conn).unwrap();
                }

                was_empty = to_payout.is_empty();

                Ok(())
            }).unwrap();
            if was_empty {
                thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    });

    drop(arc_pool);

    // start listening for events by starting a single shard
    if let Err(why) = client.start() {
        println!("An error occurred while running the client: {:?}", why);
    }
}

#[command]
fn ping(ctx: &mut Context, msg: &Message) -> CommandResult {
    msg.reply(ctx, "The use of such childish terminology to describe a professional sport played in the olympics such as table tennis is downright offensive to the athletes that have dedicated their lives to perfecting the art. Furthermore, useage of the sport as some innane way to check presence in computer networks and programs would imply that anyone can return a serve as long as they're present, which further degredates the athletes that work day and night to compete for championship tournaments throughout the world.\n\nIn response to your *serve*, I hit back a full force spinball corner return. Don't even try to hit it back.")?;

    Ok(())
}

#[command]
fn fabricate_gens(ctx: &mut Context, msg: &Message, mut args: Args) -> CommandResult {
    let how_many:i64 = args.single()?;
    if how_many <= 0 {
        Err("fuck")?;
    }
    let user:UserId;
    if args.remaining() > 0 {
        let user_str = args.single()?:String;
        user = UserId::from_command_args(ctx, msg, &user_str)?;
    }else{
        user = msg.author.id;
    }

    let conn = ctx.data.read().get::<DbPoolKey>().unwrap().get()?;

    let happened_at = chrono::Utc::now();

    conn.transaction::<_, diesel::result::Error, _>(|| {
        use diesel::prelude::*;
        for _ in 0..how_many {
            diesel::insert_into(schema::gen::table)
                .values((
                    schema::gen::owner.eq(user.0 as i64),
                    schema::gen::last_payout.eq(happened_at),
                ))
                .execute(&*conn)?;
        }

        Ok(())
    })?;

    msg.reply(&ctx, "Fabricated.")?;

    Ok(())
}

#[command]
fn fabricate_pc(ctx: &mut Context, msg: &Message, mut args: Args) -> CommandResult {
    let how_many:i64 = args.single()?;
    if how_many <= 0 {
        Err("fuck")?;
    }
    let user:UserId;
    if args.remaining() > 0 {
        let user_str = args.single()?:String;
        user = UserId::from_command_args(ctx, msg, &user_str)?;
    }else{
        user = msg.author.id;
    }

    let conn = ctx.data.read().get::<DbPoolKey>().unwrap().get()?;
    conn.transaction::<_, diesel::result::Error, _>(|| {
        use diesel::prelude::*;
        use view_schema::balance_history::dsl as bh;
        use schema::pc_transfers::dsl as pct;
        let prev_balance:i64 = view_schema::balance_history::table
          .select(bh::balance)
          .filter(bh::user.eq(user.0 as i64))
          .order(bh::happened_at.desc())
          .limit(1)
          .for_update()
          .get_result(&*conn)
          .optional()?
          .unwrap_or(0);
        
        diesel::insert_into(pct::pc_transfers).values((
            pct::quantity.eq(how_many),
            pct::to_user.eq(msg.author.id.0 as i64),
            pct::to_balance.eq(prev_balance + how_many),
            pct::happened_at.eq(chrono::Utc::now()),
            pct::message_id.eq(msg.id.0 as i64),
        )).execute(&*conn)?;

        Ok(())
    })?;

    msg.reply(&ctx, "Fabricated.")?;

    Ok(())
}

#[command]
#[aliases("balance", "inventory")]
fn balances(ctx: &mut Context, msg: &Message) -> CommandResult {
    use diesel::prelude::*;
    use schema::gen::dsl as gen;
    use view_schema::balance_history::dsl as bh;
    //use schema::pc_transfers::dsl as pct;
    

    let conn = ctx.data.read().get::<DbPoolKey>().unwrap().get()?;
    let gen_count:i64 = gen::gen.filter(gen::owner.eq(msg.author.id.0 as i64)).count().get_result(&conn)?;
    let pc_count:Option<i64> = bh::balance_history
        .select(bh::balance)
        .filter(bh::user.eq(msg.author.id.0 as i64))
        .order(bh::happened_at.desc())
        .limit(1)
        .get_result(&conn).optional()?;
    msg.channel_id.send_message(&ctx, |cm| {
        cm.embed(|e| {
            e.title("Your balances:");
            e.field("Generators", gen_count, false);
            e.field("Capital", pc_count.unwrap_or(0), false);
            e
        });
        cm
    })?;
    Ok(())
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum ItemType {
    PoliticalCapital,
    Generator,
}

const PC_NAMES :&'static [&'static str] = &["pc","politicalcapital","political-capital","capital"];
const GEN_NAMES:&'static [&'static str] = &["gen", "g", "generator", "generators", "gens"];

#[command]
#[min_args(2)]
#[max_args(3)]
fn give(ctx:&mut Context, msg:&Message, mut args:Args) -> CommandResult {
    let user_str:String = args.single()?;
    let user = UserId::from_command_args( ctx, msg, &user_str )?;
    if !ctx.cache.read().users.contains_key(&user) {
        Err("User not found")?;
    }
    let mut ty:Option<ItemType> = None;
    let mut amount:Option<u64> = None;
    for arg_result in args.iter::<String>(){
        let arg = arg_result.unwrap();
        if PC_NAMES.contains(&&*arg) {
            ty = Some(ItemType::PoliticalCapital);
        } else if GEN_NAMES.contains(&&*arg) {
            ty = Some(ItemType::Generator);
        } else {
            if let Some(idx) = arg.find(|c| !('0' <= c && c <= '9')) {
                if idx == 0 {
                    Err(format!("Invalid item type {}", arg))?;
                }
                let (count_str, ty_str) = arg.split_at(idx);
                if PC_NAMES.contains(&ty_str) {
                    ty = Some(ItemType::PoliticalCapital);
                } else if GEN_NAMES.contains(&ty_str) {
                    ty = Some(ItemType::Generator);
                } else {
                    Err(format!("Unrecognized item type {}", ty_str))?;
                }
                match count_str.parse():Result<u64,_> {
                    Err(e) => {Err(format!("Bad count {:?}", e))?;},
                    Ok(val) => {amount = Some(val);},
                }
            }else{
                match arg.parse():Result<u64, _> {
                    Err(e) => {Err(format!("Bad count {:?}", e))?;},
                    Ok(val) => {amount = Some(val);},
                }
            }
        }
    }

    if let (Some(amount), Some(ty)) = (amount, ty) {
        let conn = ctx.data.read().get::<DbPoolKey>().unwrap().get()?;

        let mut fail:Option<&'static str> = None;
        if ty == ItemType::Generator {
            conn.transaction::<_, diesel::result::Error, _>(|| {
                let gens:Vec<i64>;
                use diesel::prelude::*;
                {
                    use schema::gen::dsl::*;
                    gens = gen.select(rowid).filter(owner.eq(msg.author.id.0 as i64)).for_update().limit(amount as i64).get_results(&*conn)?;
                    if gens.len() < amount as usize {
                        fail = Some("Not enough gens");
                        return Ok(());
                    }
                    let count_updated = diesel::update(gen.filter(rowid.eq_any(&gens))).set(owner.eq(user.0 as i64)).execute(&*conn)?;
                    if count_updated < amount as usize {
                        //something went very wrong, abort!
                        return Err(diesel::result::Error::RollbackTransaction);
                    }
                }
                use schema::gen_transfers;
                #[derive(Insertable)]
                #[table_name = "gen_transfers"]
                struct GenTransfer
                {
                    from_user:i64,
                    gen:i64,
                    to_user:i64,
                    happened_at:chrono::DateTime<chrono::Utc>,
                    message_id:i64,
                }
                let mut gt = GenTransfer{
                    from_user: msg.author.id.0 as i64,
                    gen: 0,
                    to_user: user.0 as i64,
                    happened_at:chrono::Utc::now(),
                    message_id: msg.id.0 as i64,
                };
                for id in &gens {
                    gt.gen = *id;
                    diesel::insert_into(gen_transfers::table).values(&gt).execute(&*conn)?;
                }
                Ok(())
            })?;
        } else if ty == ItemType::PoliticalCapital {
            conn.transaction::<_, diesel::result::Error, _>(|| {
                use diesel::prelude::*;

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
                          .order(bh::happened_at.desc())
                          .limit(1)
                          .for_update()
                          .get_result(&*conn)
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

                use schema::pc_transfers;
                #[derive(Insertable, Debug)]
                #[table_name = "pc_transfers"]
                struct Transfer {
                    from_user:i64,
                    quantity:i64,
                    to_user:i64,
                    from_balance:i64,
                    to_balance:i64,
                    happened_at:chrono::DateTime<chrono::Utc>,
                    message_id:i64,
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
                };

                diesel::insert_into(schema::pc_transfers::table).values(&t).execute(&*conn)?;

                Ok(())
            })?;
        }
        use serenity::model::misc::Mentionable;
        if let Some(fail_msg) = fail {
            msg.reply(&ctx, fail_msg)?;
        }else{
            msg.reply(&ctx, format!(
                "Successfully transferred {} {} to {}.",
                amount,
                match ty {
                    ItemType::Generator => "generator(s)",
                    ItemType::PoliticalCapital => "political capital",
                },
                user.mention()
            ))?;
        }
    } else {
        if amount.is_none() {
            Err(format!("Amount not provided."))?;
        } else {
            Err(format!("Type not provided."))?;
        }
    }
    
    Ok(())
}