use poise::serenity_prelude::{self as serenity};
use regex::Regex;
use songbird::{input::Input, tracks::TrackHandle, Call};
use tokio::sync::Mutex;
use url::form_urlencoded;

use std::sync::Arc;

use crate::{Context, Data, Error};

const MESSAGE_READ_MAX_LENGTH: usize = 1000;
const CONNECTED_MESSAGE: &str = "お待たせ！";

#[poise::command(slash_command)]
pub async fn connect_vc(ctx: Context<'_>) -> Result<(), Error> {
    let (guild_id, channel_id) = {
        let guild = ctx.guild().expect("failed to get guild id");
        let channel_id = guild
            .voice_states
            .get(&ctx.author().id)
            .and_then(|voice_state| voice_state.channel_id);

        (guild.id, channel_id)
    };

    let connect_to = match channel_id {
        Some(channel) => channel,
        None => {
            let _ = ctx.reply("Not in a voice channel").await;

            return Ok(());
        }
    };

    let manager = songbird::get(ctx.serenity_context())
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    let _ = manager.join(guild_id, connect_to).await;
    let _ = ctx.reply("connected").await;

    if let Some(handler_lock) = manager.get(guild_id) {
        play_phrase(handler_lock, ctx.data(), CONNECTED_MESSAGE.to_string()).await?;
    }

    Ok(())
}

#[poise::command(slash_command)]
pub async fn disconnect_vc(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().expect("failed to get guild id");

    let manager = songbird::get(ctx.serenity_context())
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    if manager.get(guild_id).is_some() {
        if (manager.remove(guild_id).await).is_err() {
            let _ = ctx.reply("Failed to leave vc").await;
        }

        let _ = ctx.reply("disconnected").await;
    } else {
        let _ = ctx.reply("Not in a voice channel").await;
    }

    Ok(())
}

pub async fn on_message(
    ctx: &serenity::Context,
    message: &serenity::Message,
    data: &Data,
) -> Result<(), Error> {
    let url_pattern = Regex::new("https?://").expect("invalid as regex string");

    if message.author.bot
        || message.content.chars().count() > MESSAGE_READ_MAX_LENGTH
        || url_pattern.is_match(&message.content)
    {
        return Ok(());
    }

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    if let Some(handler_lock) = manager.get(message.guild_id.expect("failed to get guild id")) {
        // in vc
        play_phrase(handler_lock, data, message.content.clone()).await?;
    }

    Ok(())
}

/// callable when already in vc
async fn play_phrase(
    handler_lock: Arc<Mutex<Call>>,
    data: &Data,
    text: String,
) -> Result<TrackHandle, Error> {
    let base_url = &data.voicevox_api_url;

    // TODO: get from DB
    let speaker_id = 8;
    let text = form_urlencoded::byte_serialize(text.as_bytes()).collect::<String>();

    let http_client = data.http_client.clone();

    let audio_query_url = format!(
        "{}/audio_query?text={}&speaker={}",
        base_url, text, speaker_id
    );

    let audio_query = http_client
        .post(audio_query_url)
        .send()
        .await?
        .text()
        .await?;

    let synthesis_url = format!("{}/synthesis?&speaker={}", base_url, speaker_id);
    let audio: Input = http_client
        .post(synthesis_url)
        .header("Content-Type", "application/json")
        .body(audio_query)
        .send()
        .await?
        .bytes()
        .await?
        .into();

    let mut handler = handler_lock.lock().await;

    Ok(handler.play_input(audio))
}
