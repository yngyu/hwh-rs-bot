use poise::serenity_prelude as serenity;
use regex::Regex;
use songbird::input::{
    codecs::{CODEC_REGISTRY, PROBE},
    HttpRequest, Input,
};
use url::form_urlencoded;

use crate::{Context, Data, Error};

const MESSAGE_READ_MAX_LENGTH: usize = 50;

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

        let _ = ctx.reply("disconnectd").await;
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

    let api_key =
        form_urlencoded::byte_serialize(data.voicevox_api_key.as_bytes()).collect::<String>();
    // TODO: get from DB
    let speaker_id = 8;
    let text = form_urlencoded::byte_serialize(message.content.as_bytes()).collect::<String>();
    let request = format!(
        "https://deprecatedapis.tts.quest/v2/voicevox/audio/?key={}&text={}&speaker={}",
        api_key, text, speaker_id
    );

    let http_client = data.http_client.clone();

    if let Some(handler_lock) = manager.get(message.guild_id.expect("failed to get guild id")) {
        let mut handler = handler_lock.lock().await;
        let input: Input = HttpRequest::new(http_client, request).into();
        let input = input.make_playable_async(&CODEC_REGISTRY, &PROBE).await?;
        let _ = handler.play_input(input);
    }

    Ok(())
}
