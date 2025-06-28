use futures::TryStreamExt;
use mongodb::{
    bson::{self, doc, Document},
    Client, Collection,
};

use std::env::var;

use crate::Error;

pub struct Db {
    speaker_coll: Collection<Document>,
    remind_coll: Collection<Document>,
}

pub async fn build_db() -> Result<Db, Error> {
    let uri = var("MONGODB_URI")?;
    let client = Client::with_uri_str(uri).await?;
    let database = client.database("bot");
    let speaker_coll = database.collection("speakers");
    let remind_coll = database.collection("reminds");

    Ok(Db {
        speaker_coll,
        remind_coll,
    })
}

impl Db {
    pub async fn get_speaker(&self, user_id: u64) -> Result<Option<u8>, Error> {
        let filter = doc! { "user_id": user_id.to_string() };
        let speaker = self.speaker_coll.find_one(filter).await?;

        match speaker {
            Some(speaker) => Ok(Some(speaker.get_i32("speaker_id")? as u8)),
            None => Ok(None),
        }
    }

    pub async fn update_speaker(&self, user_id: u64, speaker_id: u8) -> Result<(), Error> {
        let filter = doc! { "user_id": user_id.to_string() };
        let update = doc! { "$set": { "speaker_id": (speaker_id as i32) } };

        self.speaker_coll
            .update_one(filter, update)
            .upsert(true)
            .await?;

        Ok(())
    }

    pub async fn add_new_reminder(
        &self,
        user_id: u64,
        channel_id: u64,
        content: String,
        remind_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), Error> {
        let remind_at = bson::DateTime::parse_rfc3339_str(remind_at.to_rfc3339())?;

        let reminder = doc! {
            "user_id": user_id.to_string(),
            "channel_id": channel_id.to_string(),
            "content": content,
            "remind_at": remind_at,
        };

        self.remind_coll.insert_one(reminder).await?;

        Ok(())
    }

    pub async fn get_reminders(&self) -> Result<Vec<Document>, Error> {
        let begin = chrono::Utc::now() - chrono::Duration::minutes(1);
        let begin = bson::DateTime::parse_rfc3339_str(begin.to_rfc3339())?;

        let end = bson::DateTime::now();

        let filter = doc! {
            "remind_at": {
                "$gte": begin,
                "$lte": end
            }
        };
        let cursor = self.remind_coll.find(filter).await?;
        let reminds: Vec<Document> = cursor.try_collect::<Vec<Document>>().await?;

        Ok(reminds)
    }

    pub async fn remove_reminder(&self, document: Document) -> Result<(), Error> {
        let id = document.get_object_id("_id")?;
        self.remind_coll.delete_one(doc! { "_id": id }).await?;

        Ok(())
    }

    pub async fn remove_old_reminders(&self) -> Result<Vec<Document>, Error> {
        let before = chrono::Utc::now() - chrono::Duration::minutes(1);
        let before = bson::DateTime::parse_rfc3339_str(before.to_rfc3339())?;

        let filter = doc! { "remind_at": { "$lt": before } };

        let cursor = self.remind_coll.find(filter.clone()).await?;
        let reminds: Vec<Document> = cursor.try_collect::<Vec<Document>>().await?;

        self.remind_coll.delete_many(filter).await?;

        Ok(reminds)
    }
}
