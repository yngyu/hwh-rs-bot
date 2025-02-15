#![warn(clippy::str_to_string)]

mod voice;

use poise::serenity_prelude as serenity;
use reqwest::Client;
use songbird::SerenityInit;
use std::env::var;

// Types used by all command functions
type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

// Custom user data passed to all command functions
pub struct Data {
    http_client: Client,
    voicevox_api_url: String,
    subscribing_channel_id: serenity::ChannelId,
}

async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
    // This is our custom error handler
    // They are many errors that can occur, so we only handle the ones we want to customize
    // and forward the rest to the default handler
    match error {
        poise::FrameworkError::Setup { error, .. } => panic!("Failed to start bot: {:?}", error),
        poise::FrameworkError::Command { error, ctx, .. } => {
            println!("Error in command `{}`: {:?}", ctx.command().name, error,);
        }
        error => {
            if let Err(e) = poise::builtins::on_error(error).await {
                println!("Error while handling error: {}", e)
            }
        }
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    // FrameworkOptions contains all of poise's configuration option in one struct
    // Every option can be omitted to use its default value
    let options = poise::FrameworkOptions {
        commands: vec![voice::connect_vc(), voice::disconnect_vc()],
        // The global error handler for all error cases that may occur
        on_error: |error| Box::pin(on_error(error)),
        event_handler: |ctx, event, framework, data| {
            Box::pin(event_handler(ctx, event, framework, data))
        },
        ..Default::default()
    };

    let framework = poise::Framework::builder()
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                println!("Logged in as {}", _ready.user.name);
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;

                let voicevox_api_url =
                    var("VOICEVOX_API_URL").expect("Missing `VOICEVOX_API_URL` env var");
                let subscribing_channel_id = serenity::ChannelId::new(
                    var("SUBSCRIBING_CHANNEL_ID")
                        .expect("Missing `SUBSCRIBING_CHANNEL_ID` env var")
                        .parse()
                        .expect("Failed to parse `SUBSCRIBING_CHANNEL_ID` env var"),
                );
                Ok(Data {
                    http_client: Client::new(),
                    voicevox_api_url,
                    subscribing_channel_id,
                })
            })
        })
        .options(options)
        .build();

    let token = var("DISCORD_TOKEN").expect("Missing `DISCORD_TOKEN` env var");
    let intents =
        serenity::GatewayIntents::non_privileged() | serenity::GatewayIntents::privileged();

    let client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .register_songbird()
        .await;

    client
        .expect("Error creating client")
        .start()
        .await
        .expect("Failed to start bot");
}

async fn event_handler(
    ctx: &serenity::Context,
    event: &serenity::FullEvent,
    _framework: poise::FrameworkContext<'_, Data, Error>,
    data: &Data,
) -> Result<(), Error> {
    if let serenity::FullEvent::Message { new_message } = event {
        if new_message.channel_id == data.subscribing_channel_id {
            let _ = voice::on_message(ctx, new_message, data).await;
        }
    }

    Ok(())
}
