use chrono::prelude::*;
use env_logger;
use exitfailure::ExitFailure;
use failure::Error;
use failure::{self, ResultExt};
use log::{debug, info, warn};
use reqwest;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use structopt::StructOpt;
use url::Url;

mod db;

#[derive(Debug, StructOpt)]
struct Opts {
    /// Path where fetchmixes will put its Sqlite database.
    #[structopt(
        short = "d",
        long = "database",
        default_value = "./fetchmixes.db",
        parse(from_os_str)
    )]
    db_path: PathBuf,

    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(Debug, StructOpt)]
enum Command {
    /// Crawl a DJ's page on Mixcloud. Just saves a list of sets in the fetchmixes database; does _not_ download audio or setlists.
    #[structopt(name = "crawl-dj")]
    CrawlDj { dj_username: String },
}

fn get_csrf_token(client: &reqwest::Client, profile_url: &str) -> Result<String, Error> {
    let resp = client.get(profile_url).send()?;
    let csrfcookie = resp.cookies().find(|c| c.name() == "csrftoken").unwrap();
    Ok(csrfcookie.value().to_string())
}

#[derive(Debug, Deserialize)]
pub struct CloudcastCovers {
    extra_large: String,
}

#[derive(Debug, Deserialize)]
pub struct Cloudcast {
    play_count: i64,
    url: String,
    pictures: CloudcastCovers,
    created_time: DateTime<Utc>,
    updated_time: DateTime<Utc>,
}

fn main() -> Result<(), ExitFailure> {
    env_logger::init();
    let opts = Opts::from_args();

    let mut db_conn = db::init(opts.db_path)?;

    match opts.cmd {
        Command::CrawlDj { dj_username } => {
            let frontend_base = Url::parse("https://www.mixcloud.com/")?;
            let api_base = Url::parse("https://api.mixcloud.com/")?;
            let graphql_endpoint = Url::parse("https://www.mixcloud.com/graphql")?;

            let profile_page = frontend_base.join(&dj_username)?;
            let api_page = api_base.join(&dj_username)?;

            let client = reqwest::Client::new();

            // -- get a CSRF token
            let csrf = get_csrf_token(&client, profile_page.as_str())?;
            info!("Got CSRF token: {}", csrf);

            // -- get mixcount
            let dj_info: Value = client.get(api_page.as_str()).send()?.json()?;
            let n_mixes = dj_info["cloudcast_count"].as_i64().unwrap();

            info!("Attempting to fetch {} mixes", n_mixes);

            // -- get ID from UserStatsCard
            let payload_userstatscard = json! {
                {"id":"q59","query":"query UserStatsCard($lookup_0:UserLookup!,$first_1:Int!) {userLookup(lookup:$lookup_0) {id,...F1}} fragment F0 on Stats {comments {totalCount},favorites {totalCount},reposts {totalCount},plays {totalCount},minutes {totalCount},__typename} fragment F1 on User {isViewer,isUploader,username,hasProFeatures,_uploads1No11a:uploads(first:$first_1) {edges {node {id},cursor},pageInfo {hasNextPage,hasPreviousPage}},stats {...F0},id}","variables":{"lookup_0":{"username":"Grundfunk"},"first_1":1}}
            };
            let usercard: Value = client
                .post(graphql_endpoint.as_str())
                .header("X-CSRFToken", csrf.as_str())
                .header("Referer", profile_page.as_str())
                .header("Cookie", format!("csrftoken={}", csrf))
                .json(&payload_userstatscard)
                .send()?
                .json()?;

            let dj_id = usercard["data"]["userLookup"]["id"].as_str().unwrap();
            info!("DJ has internal user id of: {}", dj_id);
            let dj_pk_id = db::upsert_dj(&mut db_conn, &dj_username, &dj_id)?;
            info!("Committed DJ metadata to sqlite; djs(id) is {}", dj_pk_id);

            info!("Fetching full list of DJ's sets");
            let mut cloudcasts: Vec<Cloudcast> = Vec::new();
            let mut cloudcasts_api = api_page.clone();
            cloudcasts_api
                .path_segments_mut()
                .unwrap()
                .push("cloudcasts");
            loop {
                debug!("fetching url {}", cloudcasts_api.as_str());
                let resp: Value = client.get(cloudcasts_api.as_str()).send()?.json()?;
                let ccast_list: Vec<Cloudcast> = serde_json::from_value(resp["data"].clone())?;
                cloudcasts.extend(ccast_list);

                match resp["paging"]["next"].as_str() {
                    Some(next_page) => cloudcasts_api = Url::parse(next_page)?,
                    None => {
                        debug!("hit end of cloudcasts list pagination, breaking");
                        break;
                    }
                }
            }

            db::insert_api_cloudcasts(&mut db_conn, &cloudcasts[..])?;
        }
    }

    Ok(())
}
