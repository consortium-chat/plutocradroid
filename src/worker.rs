use std::env;
use std::time::Duration;
use std::sync::Arc;

use super::tasks;

pub async fn main() {
    let http = serenity::http::Http::new_with_token(&env::var("DISCORD_TOKEN").expect("token"));
    let raw_pool = diesel::r2d2::Builder::new().build(
        diesel::r2d2::ConnectionManager::<diesel::PgConnection>::new(
            &env::var("DATABASE_URL").expect("DATABASE_URL expected")
        )
    ).expect("could not build DB pool");
    let arc_pool = Arc::new(raw_pool);
    loop {
        tasks::process_motion_completions(&arc_pool, &http).await.expect("Failed to process motion completions");
        tasks::create_auto_auctions(&arc_pool, &http).await.expect("Failed create_auto_auctions");
        tasks::process_auctions(&arc_pool, &http).await.expect("Failed process_auctions");
        let blocking_arc = Arc::clone(&arc_pool);
        tokio::task::spawn_blocking(move ||{
            let conn = blocking_arc.get().unwrap();
            tasks::process_generators(&*conn).expect("Failed to process generators");
            tasks::update_last_task_run(&*conn).expect("Failed update_last_task_run");
        }).await.unwrap();

        tokio::time::sleep(Duration::from_millis(5000)).await;
    }
}