use poise::serenity_prelude::futures::StreamExt;
use poise::serenity_prelude::{self as serenity, json, CacheHttp, EditMessage, Message, User};
use regex::Regex;
use std::env::var;
use std::str;
use std::sync::Arc;

use crate::Error;

const MODEL: &str = "llama3.2:3b";
const MAX_MESSAGE_LENGTH: usize = 2000;

pub struct Chat {
    http_client: Arc<reqwest::Client>,
    llm_api_url: String,
    bot: Arc<User>,
    mention_pattern: Regex,
}

pub fn build_chat(http_client: Arc<reqwest::Client>, bot: Arc<User>) -> Result<Chat, Error> {
    let mention_pattern = Regex::new(&format!("<@{}>[\\s　]*", bot.id))?;

    Ok(Chat {
        http_client,
        llm_api_url: var("LLM_API_URL")?,
        bot,
        mention_pattern,
    })
}

impl Chat {
    pub async fn on_message(
        &self,
        ctx: &serenity::Context,
        message: &serenity::Message,
    ) -> Result<(), Error> {
        // ignore messsages from myself
        if message.author.id == self.bot.id {
            return Ok(());
        }

        if !message.mentions_user(Arc::as_ref(&self.bot)) {
            return Ok(());
        }

        let mut chain = vec![];
        self.get_reply_chain(ctx, message, &mut chain).await?;

        let system_message = json::json!({
            "role": "system",
            "content": "あなたはDiscordで使用されているアシスタントbotです。質問に対しては簡潔な回答を心掛けてください。",
        });
        let messages = chain
            .iter()
            .map(|m| {
                if m.author.id == self.bot.id {
                    json::json!({
                        "role": "assistant",
                        "content": m.content,
                    })
                } else {
                    json::json!({
                        "role": "user",
                        "content": self.delete_mention_to_myself(m),
                    })
                }
            })
            .collect::<Vec<json::Value>>();
        let messages = vec![system_message]
            .into_iter()
            .chain(messages)
            .collect::<Vec<json::Value>>();

        let chat_completion_url = format!("{}/api/chat", self.llm_api_url);
        let mut response = self
            .http_client
            .post(&chat_completion_url)
            .header("Content-Type", "application/json")
            .body(
                json::json!({
                    "model": MODEL,
                    "messages": messages,
                })
                .to_string(),
            )
            .send()
            .await?
            .bytes_stream();

        let mut my_message = Message::default();
        let mut count = MAX_MESSAGE_LENGTH;
        let mut reply = String::new();

        let mut stream_buffer: Vec<u8> = Vec::new();
        let mut reply_buffer = String::new();

        while let Some(bytes) = response.next().await {
            let bytes = bytes?.clone();
            let chunks = bytes.split(|&x| x == b'\n').collect::<Vec<&[u8]>>();
            let mut done = false;

            for chunk in chunks {
                stream_buffer.extend_from_slice(chunk);
                if let Ok(piece) = json::from_slice::<json::Value>(&stream_buffer) {
                    stream_buffer.clear();
                    reply_buffer.push_str(piece["message"]["content"].as_str().unwrap());
                    if piece["done"].as_bool().unwrap_or(false) {
                        done = true;
                        break;
                    }
                }
            }

            if done || reply_buffer.chars().count() >= 100 {
                if (count + reply_buffer.chars().count()) >= MAX_MESSAGE_LENGTH {
                    reply = reply_buffer.clone();
                    my_message = message.reply(ctx, &reply).await?;
                    count = reply.chars().count();
                } else {
                    reply.push_str(&reply_buffer);
                    my_message
                        .edit(ctx, EditMessage::new().content(&reply))
                        .await?;
                    count += reply_buffer.chars().count();
                }

                reply_buffer.clear();
            }
        }

        Ok(())
    }

    async fn get_reply_chain(
        &self,
        ctx: &serenity::Context,
        message: &serenity::Message,
        chain: &mut Vec<serenity::Message>,
    ) -> Result<(), Error> {
        if let Some(referenced_message) = &message.referenced_message {
            let message = ctx
                .http()
                .get_message(referenced_message.channel_id, referenced_message.id)
                .await?;

            Box::pin(self.get_reply_chain(ctx, &message, chain)).await?;
        }

        chain.push(message.clone());

        Ok(())
    }

    fn delete_mention_to_myself(&self, message: &serenity::Message) -> String {
        self.mention_pattern
            .replace_all(&message.content, "")
            .to_string()
    }
}
