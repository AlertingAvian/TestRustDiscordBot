use core::panic;
use std::collections::HashSet;
use std::env;
use std::sync::Arc;

use rand::Rng;
use serenity::async_trait;
use serenity::builder::{CreateComponents, CreateEmbed};
use serenity::client::bridge::gateway::ShardManager;
use serenity::http::Http;
use serenity::model::application::command::{Command, CommandOptionType};
use serenity::model::application::interaction::application_command::CommandDataOptionValue;
use serenity::model::application::interaction::{Interaction, InteractionResponseType};
use serenity::model::event::ResumedEvent;
use serenity::model::gateway::Ready;
use serenity::model::id::GuildId;
use serenity::prelude::*;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

pub struct ShardManagerContainer;

impl TypeMapKey for ShardManagerContainer {
    type Value = Arc<Mutex<ShardManager>>;
}

pub struct Owners;

impl TypeMapKey for Owners {
    type Value = Arc<RwLock<HashSet<serenity::model::prelude::UserId>>>;
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

        let guild_commands = GuildId::set_application_commands(&guild_id, &ctx.http, |commands| {
            commands.create_application_command(|command| {
                command
                    .name("shutdown")
                    .description("Owner only, shuts the bot down.")
            })
        })
        .await;
        debug!(
            "Activated the following guild slash commands: {:?}",
            guild_commands
        );

        let global_commands = Command::set_global_application_commands(&ctx.http, |commands| {
            commands
                .create_application_command(|command| {
                    command.name("info").description("Get info about the bot")
                })
                .create_application_command(|command| {
                    command.name("xkcd").description("Get random xkcd comic.")
                })
        })
        .await;
        debug!(
            "Activated the following global slash commands: {:?}",
            global_commands
        )
    }

    async fn resume(&self, _: Context, _: ResumedEvent) {
        info!("Resumed")
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            debug!("Recived command interaction: {:?}", command);

            let (respond_ephemeral, content, embeds, components) = match command.data.name.as_str()
            {
                "info" => {
                    let content = "Test bot created by AlertingAvian".to_string();

                    (true, content, vec![], CreateComponents::default())
                }
                "xkcd" => {
                    let mut rng = rand::thread_rng();

                    let content = format!("https://xkcd.com/{}/", rng.gen_range(1..2667)); // the lazy way to do it

                    (false, content, vec![], CreateComponents::default())
                }
                "shutdown" => {
                    info!("shutdown command invoked");
                    let mut content = String::new(); // it is used, i think it might be angry because of the return statement
                    let owners = {
                        let data_read = ctx.data.read().await;
                        let owners_lock = data_read
                            .get::<Owners>()
                            .expect("Expected Owners in TypeMap")
                            .clone();

                        let owners = owners_lock.read().await;
                        owners.clone()
                    };
                    if owners.contains(&command.user.id) {
                        let data = ctx.data.read().await;

                        if let Some(manager) = data.get::<ShardManagerContainer>() {
                            if let Err(why) = command
                                .create_interaction_response(&ctx.http, |response| {
                                    response
                                        .kind(InteractionResponseType::ChannelMessageWithSource)
                                        .interaction_response_data(|message| {
                                            message.ephemeral(true).content("Shutting down")
                                        })
                                })
                                .await
                            {
                                error!("Cannot respond to slash command: {}", why);
                            }
                            manager.lock().await.shutdown_all().await;
                            return;
                        } else {
                            content = "There was a problem getting the shard manager".to_string();
                        }
                    } else {
                        content = "You shouldn't have this.".to_string();
                        warn!("User with insufficient permissions invoked Shutdown command!");
                    }
                    (true, content, vec![], CreateComponents::default())
                }
                _ => {
                    let content = "".to_string();

                    let mut embed = CreateEmbed::default();
                    embed.title("Error");
                    embed.description("Not Implemented");
                    embed.color((255, 0, 0));

                    (true, content, vec![embed], CreateComponents::default())
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
                error!("Cannot respond to slash command: {}", why);
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
    // TODO: Have log to stdout and to a file

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

    {
        let mut data = client.data.write().await;
        data.insert::<Owners>(Arc::new(RwLock::new(owners)))
    }

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
