use env_logger;
use exitfailure::ExitFailure;
use failure::Error;
use failure::{self, ResultExt};
use log::{debug, info, warn};
use reqwest;
use serde_json::{json, Value};
use std::fs::File;
use std::path::PathBuf;
use structopt::StructOpt;
use url::Url;

mod data;
mod db;

use data::*;

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

    #[structopt(name = "fetch-setlists")]
    FetchSetlists { dj_username: String },
}

fn get_csrf_token(client: &reqwest::Client, profile_url: &str) -> Result<String, Error> {
    let resp = client.get(profile_url).send()?;
    let csrfcookie = resp.cookies().find(|c| c.name() == "csrftoken").unwrap();
    Ok(csrfcookie.value().to_string())
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
                {
                    "id":"q59",
                    "query": include_str!("../queries/UserStatsCard.graphql"),
                    "variables":{"lookup_0":{"username":"Grundfunk"},"first_1":1}
                }
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

            // -- get sets from UserUploadsPageQuery
            let mut all_casts: Vec<Cloudcast> = Vec::new();
            let mut cursor: Option<String> = None;
            for i in 1.. {
                debug!(
                    "running UserUploadsPageQuery through /graphql for page {} (afterCursor={:?})",
                    i, cursor
                );
                let payload = json! {
                    {
                        "id":"q88",
                        "query": include_str!("../queries/UserUploadsPageQuery.graphql"),
                        "variables":{"first_0":20,"orderBy_1":"LATEST","afterCursor":cursor, "userId": dj_id}
                    }
                };
                let query_result: Value = client
                    .post(graphql_endpoint.as_str())
                    .header("X-CSRFToken", csrf.as_str())
                    .header("Referer", profile_page.as_str())
                    .header("Cookie", format!("csrftoken={}", csrf))
                    .json(&payload)
                    .send()?
                    .json()?;

                let q_r: QueryResponse<UserUploadsData> =
                    serde_json::from_value(query_result.clone())?;
                cursor = Some(q_r.data.uploads.last().unwrap().cursor.clone());
                all_casts.extend(q_r.data.uploads.into_iter().map(|u| u.cloudcast));

                let page_info: PageInfo = serde_json::from_value(
                    query_result["data"]["user"]["uploads"]["pageInfo"].clone(),
                )?;
                if !page_info.has_next_page {
                    break;
                }
            }

            db::insert_api_cloudcasts(&mut db_conn, dj_pk_id, &all_casts)?;
        }
        Command::FetchSetlists { dj_username } => {}
    }

    Ok(())
}
