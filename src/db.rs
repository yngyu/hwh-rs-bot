use mongodb::{
    bson::{doc, Document},
    Client, Collection,
};

use std::env::var;

use crate::Error;

pub struct Db {
    speaker_coll: Collection<Document>,
}

pub async fn build_db() -> Result<Db, Error> {
    let uri = var("MONGODB_URI")?;
    let client = Client::with_uri_str(uri).await?;
    let database = client.database("bot");
    let speaker_coll = database.collection("speakers");

    Ok(Db { speaker_coll })
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
}
