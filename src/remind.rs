use chrono::{Datelike, TimeZone, Timelike};
use poise::serenity_prelude::{self as serenity};
use std::sync::Arc;

use tokio::time::{interval, Duration};

use regex::Regex;

use mongodb::bson::doc;

use chrono_tz::Asia::Tokyo;

use crate::db::Db;
use crate::{Context, Error};

#[derive(Clone)]
pub struct Remind {
    db: Arc<Db>,
    patterns: Vec<Regex>,
}

pub fn build_remind(db: Arc<Db>) -> Result<Remind, Error> {
    const PATTERNS: [&str; 2] = [
        r"^(?:(\d+)/(\d+)\s+)*(\d+):(\d+)\s+(.+)$",
        r"^(?:(\d+)h)*(?:(\d+)m)*\s+(.+)$",
    ];

    let patterns: Vec<Regex> = PATTERNS
        .iter()
        .map(|&pattern| Regex::new(pattern).expect("Failed to compile regex"))
        .collect();

    Ok(Remind { db, patterns })
}

impl Remind {
    pub async fn remind(&self, ctx: Context<'_>, message: String) -> Result<(), Error> {
        let channel_id: u64 = ctx.channel_id().into();
        let user_id: u64 = ctx.author().id.into();

        let now_jst = chrono::Utc::now().with_timezone(&Tokyo);
        let final_dst;
        let content;

        if let Some(captures) = self.patterns[0].captures(&message) {
            let month = captures.get(1).map_or(Ok(0), |m| m.as_str().parse());
            let day = captures.get(2).map_or(Ok(0), |m| m.as_str().parse());
            let hour = captures.get(3).map_or(Ok(0), |m| m.as_str().parse());
            let minute = captures.get(4).map_or(Ok(0), |m| m.as_str().parse());
            content = captures
                .get(5)
                .map_or_else(String::new, |m| m.as_str().into());

            if month.is_err() || day.is_err() || hour.is_err() || minute.is_err() {
                ctx.say("Invalid date or time format.").await?;
                return Ok(());
            }

            let month = month.unwrap_or(now_jst.month());
            let day = day.unwrap_or(now_jst.day());
            let hour = hour.unwrap_or(now_jst.hour());
            let minute = minute.unwrap_or(now_jst.minute());

            if month != 0 && day != 0 {
                let year = now_jst.year();

                let native_dt = chrono::NaiveDate::from_ymd_opt(year, month, day)
                    .and_then(|date| date.and_hms_opt(hour, minute, 0))
                    .ok_or("Invalid date or time".to_owned())?;
                let dst = Tokyo
                    .from_local_datetime(&native_dt)
                    .single()
                    .ok_or("Invalid date or time".to_owned())?;

                if dst < now_jst {
                    final_dst = dst
                        .checked_add_months(chrono::Months::new(12))
                        .ok_or("Failed to add month")?;
                } else {
                    final_dst = dst;
                }
            } else {
                let year = now_jst.year();
                let month = now_jst.month();
                let day = now_jst.day();

                let native_dt = chrono::NaiveDate::from_ymd_opt(year, month, day)
                    .and_then(|date| date.and_hms_opt(hour, minute, 0))
                    .ok_or("Invalid date or time".to_owned())?;
                let dst = Tokyo
                    .from_local_datetime(&native_dt)
                    .single()
                    .ok_or("Invalid date or time".to_owned())?;

                if dst < now_jst {
                    final_dst = dst
                        .checked_add_days(chrono::Days::new(1))
                        .ok_or("Failed to add day")?;
                } else {
                    final_dst = dst;
                }
            }
        } else if let Some(captures) = self.patterns[1].captures(&message) {
            let hours = captures.get(1).map_or(0, |m| m.as_str().parse().unwrap());
            let minutes = captures.get(2).map_or(0, |m| m.as_str().parse().unwrap());
            content = captures
                .get(3)
                .map_or_else(String::new, |m| m.as_str().into());

            final_dst =
                now_jst + chrono::Duration::hours(hours) + chrono::Duration::minutes(minutes);
        } else {
            ctx.say("Invalid time format.").await?;
            return Ok(());
        }

        self.db
            .add_new_reminder(user_id, channel_id, content.clone(), final_dst.to_utc())
            .await?;

        ctx.say(format!(
            "Set a reminder for {} {}",
            final_dst.format("%Y-%m-%d %H:%M:%S"),
            content
        ))
        .await?;

        Ok(())
    }

    pub async fn invoke_reminders(&self, ctx: &serenity::Context) -> Result<(), Error> {
        let mut interval = interval(Duration::from_secs(10));
        loop {
            interval.tick().await;

            if let Err(e) = self.check_reminders(ctx).await {
                log::error!("Error checking reminders: {e}");
            }
        }
    }

    async fn check_reminders(&self, ctx: &serenity::Context) -> Result<(), Error> {
        let reminders = self.db.get_reminders().await?;

        for reminder in reminders {
            let channel_id = reminder.get_str("channel_id")?.parse::<u64>()?;
            let user_id = reminder.get_str("user_id")?.parse::<u64>()?;
            let channel = serenity::ChannelId::new(channel_id);

            channel
                .say(
                    ctx,
                    format!("<@{}> {}", user_id, reminder.get_str("content")?),
                )
                .await?;

            self.db.remove_reminder(reminder).await?;
        }

        let removed = self.db.remove_old_reminders().await?;
        for reminder in removed {
            let channel_id = reminder.get_str("channel_id")?.parse::<u64>()?;
            let user_id = reminder.get_str("user_id")?.parse::<u64>()?;
            let channel = serenity::ChannelId::new(channel_id);
            let content = reminder.get_str("content")?;
            let remind_at = reminder
                .get_datetime("remind_at")?
                .try_to_rfc3339_string()?;

            channel
                .say(
                    ctx,
                    format!("<@{user_id}>: Reminder {content} at {remind_at} was skipped."),
                )
                .await?;
        }

        Ok(())
    }
}

/// Set reminder
#[poise::command(slash_command)]
pub async fn remind(ctx: Context<'_>, message: String) -> Result<(), Error> {
    ctx.data().remind.remind(ctx, message).await
}
