use chrono::prelude::*;
use serde::{Deserialize, Deserializer};

#[derive(Deserialize, Debug)]
pub struct Upload {
    #[serde(rename = "node")]
    pub cloudcast: Cloudcast,
    pub cursor: String,
}

#[derive(Debug, Deserialize)]
pub struct UserUploadsData {
    #[serde(rename = "user", deserialize_with = "user_uploads_deser")]
    pub uploads: Vec<Upload>,
}

fn user_uploads_deser<'de, D>(deserializer: D) -> Result<Vec<Upload>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    struct User {
        uploads: Uploads,
    }

    #[derive(Deserialize)]
    struct Uploads {
        edges: Vec<Upload>,
    }

    Ok(User::deserialize(deserializer)?
        .uploads
        .edges)
}

#[derive(Debug, Deserialize)]
pub struct QueryResponse<D> {
    pub data: D,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamInfo {
    pub dash_url: Option<String>,
    pub hls_url: Option<String>,
    pub url: String,
    pub uuid: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cloudcast {
    pub id: String,
    pub audio_length: u32,
    pub name: String,
    pub description: String,
    /// Stream info. None if not available in the requester's country (could perhaps retry via a VPN 😉).
    pub stream_info: Option<StreamInfo>,
    pub publish_date: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    pub end_cursor: String,
    pub has_next_page: bool,
}
