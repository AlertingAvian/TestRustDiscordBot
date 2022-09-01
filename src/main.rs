use core::panic;
use std::collections::HashSet;
use std::env;
use std::sync::Arc;

use serenity::async_trait;
use serenity::client::bridge::gateway::ShardManager;
use serenity::http::Http;
use serenity::model::event::ResumedEvent;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use tracing::{debug, error, info};

pub struct ShardManagerContainer;

impl TypeMapKey for ShardManagerContainer {
    type Value = Arc<Mutex<ShardManager>>;
}


struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        info!("Connected as {}", ready.user.name);
        debug!("{}", ready.user.id)
    }

    async fn resume(&self, _: Context, _: ResumedEvent) {
        info!("Resumed")
    }
    // TODO: add interaction (slash commands, et. al.) events
}

// will not be using the normal message commands

#[tokio::main]
async fn main() {
    // load env variables at ./.env relative to CWD
    dotenv::dotenv().expect("Failed to load .env file");

    // init logger to use environment vars
    tracing_subscriber::fmt::init();
    // let file_appender = tracing_appender::rolling::hourly("./", "bot.log");
    // let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    // tracing_subscriber::fmt() // dont know how to get it to set with the env var when file logging
    //     .with_writer(non_blocking)
    //     .with_writer(std::io::stderr)
    //     .init();

    // load token
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in environment");


    
    // not sure what this is, following examples
    let http = Http::new(&token);

    // Fetch bot's owners and id
    let (owners, _bot_id) = match http.get_current_application_info().await {
        Ok(info) => {
            let mut owners = HashSet::new();
            owners.insert(info.owner.id);

            (owners, info.id)
        },
        Err(why) => panic!("Could not access application info: {:?}", why),
    };
    debug!("Owners: {:?}", owners);
    // framework not needed because not using chat commands

    let intents = GatewayIntents::GUILD_MEMBERS | GatewayIntents::GUILDS;

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .await
        .expect("Error creating client");
    
    {
        let mut data = client.data.write().await;
        data.insert::<ShardManagerContainer>(client.shard_manager.clone());
    }

    let shard_manager = client.shard_manager.clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.expect("Could not register ctrl+c handler");
        shard_manager.lock().await.shutdown_all().await;
    });

    if let Err(why) = client.start().await {
        error!("Client error: {:?}", why)
    }
}
