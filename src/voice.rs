use poise::serenity_prelude::{self as serenity, json, CreateAttachment, UserId};
use poise::CreateReply;
use regex::Regex;
use songbird::{input::Input, tracks::TrackHandle, Call};
use tokio::sync::Mutex;
use url::form_urlencoded;

use std::collections::BTreeMap;
use std::env::var;
use std::sync::Arc;

use crate::db::Db;
use crate::{Context, Error};

const MESSAGE_READ_MAX_LENGTH: usize = 1000;
const CONNECTED_MESSAGE: &str = "お待たせ！";
const DEFAULT_SPEAKER_ID: u8 = 8;

pub struct Voice {
    http_client: Arc<reqwest::Client>,
    voicevox_api_url: String,
    subscribgin_chanel_id: serenity::ChannelId,
    db: Arc<Db>,
}

pub fn build_voice(http_client: Arc<reqwest::Client>, db: Arc<Db>) -> Result<Voice, Error> {
    Ok(Voice {
        http_client,
        voicevox_api_url: var("VOICEVOX_API_URL")?,
        subscribgin_chanel_id: serenity::ChannelId::new(var("SUBSCRIBING_CHANNEL_ID")?.parse()?),
        db,
    })
}

impl Voice {
    pub async fn connect_vc(&self, ctx: Context<'_>) -> Result<(), Error> {
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
            self.play_phrase(
                handler_lock,
                &ctx.author().id,
                &CONNECTED_MESSAGE.to_owned(),
            )
            .await?;
        }

        Ok(())
    }

    pub async fn disconnect_vc(&self, ctx: Context<'_>) -> Result<(), Error> {
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
        &self,
        ctx: &serenity::Context,
        message: &serenity::Message,
    ) -> Result<(), Error> {
        if message.channel_id != self.subscribgin_chanel_id {
            return Ok(());
        }

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
            self.play_phrase(handler_lock, &message.author.id, &message.content)
                .await?;
        }

        Ok(())
    }

    /// callable when already in vc
    async fn play_phrase(
        &self,
        handler_lock: Arc<Mutex<Call>>,
        user_id: &UserId,
        text: &String,
    ) -> Result<TrackHandle, Error> {
        let speaker_id = self.get_vc(user_id.get()).await?;
        let text = form_urlencoded::byte_serialize(text.as_bytes()).collect::<String>();

        let audio_query_url = format!(
            "{}/audio_query?text={}&speaker={}",
            &self.voicevox_api_url, text, speaker_id
        );

        let audio_query = self
            .http_client
            .post(audio_query_url)
            .send()
            .await?
            .text()
            .await?;

        let synthesis_url = format!(
            "{}/synthesis?&speaker={}",
            &self.voicevox_api_url, speaker_id
        );
        let audio: Input = self
            .http_client
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

    pub async fn show_vc(&self, ctx: Context<'_>) -> Result<(), Error> {
        let user_id = ctx.author().id.get();
        let speaker_id = self.get_vc(user_id).await?;
        let speakers = self.get_vcs().await?;
        let speaker = speakers
            .get(&speaker_id)
            .expect("Failed to get current speaker")
            .to_string();

        ctx.reply(speaker).await?;

        Ok(())
    }

    async fn get_vc(&self, user_id: u64) -> Result<u8, Error> {
        let speaker_id = match self.db.get_speaker(user_id).await? {
            Some(speaker_id) => speaker_id,
            None => {
                self.db.update_speaker(user_id, DEFAULT_SPEAKER_ID).await?;
                DEFAULT_SPEAKER_ID
            }
        };

        Ok(speaker_id)
    }

    pub async fn set_vc(&self, ctx: Context<'_>, id: u8) -> Result<(), Error> {
        let user_id = ctx.author().id.get();
        let speakers = self.get_vcs().await?;

        match speakers.get(&id) {
            Some(speaker) => {
                self.db.update_speaker(user_id, id).await?;

                ctx.reply(format!("Your speaker has been set as {speaker}"))
                    .await?;
            }
            None => {
                ctx.reply("The id is invalid.").await?;
            }
        }

        Ok(())
    }

    pub async fn show_vcs_info(&self, ctx: Context<'_>) -> Result<(), Error> {
        let seakers = self.get_vcs().await?;
        let json = json::to_string_pretty(&seakers)?;

        let attachment = CreateAttachment::bytes(json.as_bytes(), "speakers.json");
        let reply = CreateReply::default()
            .attachment(attachment)
            .ephemeral(true);

        ctx.send(reply).await?;

        Ok(())
    }

    pub async fn show_vc_info(&self, ctx: Context<'_>, id: u8) -> Result<(), Error> {
        let speakers = self.get_vcs().await?;

        let speaker = speakers.get(&id);

        if let Some(speaker) = speaker {
            ctx.reply(speaker.to_string()).await?;
        } else {
            ctx.reply("Not found").await?;
        }

        Ok(())
    }

    pub async fn get_vcs(&self) -> Result<BTreeMap<u8, json::Value>, Error> {
        let speakers_query_url = format!("{}/speakers", self.voicevox_api_url);

        let speakers_str = self
            .http_client
            .get(speakers_query_url)
            .send()
            .await?
            .text()
            .await?;

        let speakers: json::Value = json::from_str(&speakers_str)?;
        let speakers = speakers.as_array().expect("failed to parse speakers");

        let mut speakers_info: BTreeMap<u8, json::Value> = BTreeMap::new();

        speakers.iter().for_each(|speaker| {
            let name = speaker
                .get("name")
                .expect("failed to parse name")
                .as_str()
                .expect("failed to parse name");

            speaker
                .get("styles")
                .expect("failed to parse styles")
                .as_array()
                .expect("failed to parse styles")
                .iter()
                .for_each(|style| {
                    let style_name = style
                        .get("name")
                        .expect("failed to parse name")
                        .as_str()
                        .expect("failed to parse name");
                    let style_id = style
                        .get("id")
                        .expect("failed to parse id")
                        .as_u64()
                        .expect("failed to parse id") as u8;

                    speakers_info.entry(style_id).or_insert(json::json!({
                        "name": name,
                        "style": style_name
                    }));
                });
        });

        Ok(speakers_info)
    }
}

/// Connect to the voice channel the user is in
#[poise::command(slash_command)]
pub async fn connect_vc(ctx: Context<'_>) -> Result<(), Error> {
    ctx.data().voice.connect_vc(ctx).await
}

/// Disconnect from the voice channel
#[poise::command(slash_command)]
pub async fn disconnect_vc(ctx: Context<'_>) -> Result<(), Error> {
    ctx.data().voice.disconnect_vc(ctx).await
}

/// Show vc
#[poise::command(slash_command)]
pub async fn show_vc(ctx: Context<'_>) -> Result<(), Error> {
    ctx.data().voice.show_vc(ctx).await
}

/// Set vc
#[poise::command(slash_command)]
pub async fn set_vc(ctx: Context<'_>, #[description = "id"] id: u8) -> Result<(), Error> {
    ctx.data().voice.set_vc(ctx, id).await
}

/// Show all speakers info
#[poise::command(slash_command)]
pub async fn show_vcs_info(ctx: Context<'_>) -> Result<(), Error> {
    ctx.data().voice.show_vcs_info(ctx).await
}

/// Show speaker info
#[poise::command(slash_command)]
pub async fn show_vc_info(ctx: Context<'_>, #[description = "id"] id: u8) -> Result<(), Error> {
    ctx.data().voice.show_vc_info(ctx, id).await
}
