use core::panic;
use std::collections::HashSet;
use std::env;
use std::sync::Arc;

use serenity::async_trait;
use serenity::builder::{CreateEmbed, CreateComponents};
use serenity::client::bridge::gateway::ShardManager;
use serenity::http::Http;
use serenity::model::application::command::{Command, CommandOptionType};
use serenity::model::application::interaction::application_command::CommandDataOptionValue;
use serenity::model::application::interaction::{Interaction, InteractionResponseType};
use serenity::model::event::ResumedEvent;
use serenity::model::gateway::Ready;
use serenity::model::id::GuildId;
use serenity::prelude::*;
use tracing::{debug, error, info};

pub struct ShardManagerContainer;

impl TypeMapKey for ShardManagerContainer {
    type Value = Arc<Mutex<ShardManager>>;
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("Connected as {}", ready.user.name);
        debug!("ID: {}", ready.user.id);

        let guild_id = GuildId(
            env::var("GUILD_ID")
                .expect("Expected GUILD_ID in environment")
                .parse()
                .expect("GUILD_ID must be an integer"),
        );

        let commands = GuildId::set_application_commands(&guild_id, &ctx.http, |commands|{
            commands
                .create_application_command(|command| {
                    command
                        .name("info")
                        .description("Get info about the bot")
                })
        })
        .await;
        debug!("Activated the following guild slash commands: {:?}", commands)

        // create and activate global slash commands here
    }

    async fn resume(&self, _: Context, _: ResumedEvent) {
        info!("Resumed")
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            debug!("Recived command interaction: {:?}", command);

            let (respond_ephemeral, content, embeds, components) = match command.data.name.as_str() {
                _ => {
                    let content = "".to_string();

                    let mut embed = CreateEmbed::default();
                    embed.title("Error");
                    embed.description("Not Implemented");
                    embed.color((255, 0, 0));

                    let components = CreateComponents::default(); // default components don't inclulde any
                    
                    (true, content, vec![embed], components)
                }
            };
            if let Err(why) = command
                .create_interaction_response(&ctx.http, |response| {
                    response
                        .kind(InteractionResponseType::ChannelMessageWithSource)
                        .interaction_response_data(|message| {
                            message
                                .ephemeral(respond_ephemeral)
                                .content(content)
                                .set_embeds(embeds)
                                .set_components(components)
                                //.add_files() // will probably want to work in support for files at some point
                        })
                })
                .await
            {
                error!("Cannot respond to slash command: {}", why)
            }
        }
    }
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
        }
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
        tokio::signal::ctrl_c()
            .await
            .expect("Could not register ctrl+c handler");
        shard_manager.lock().await.shutdown_all().await;
    });

    if let Err(why) = client.start().await {
        error!("Client error: {:?}", why)
    }
}
