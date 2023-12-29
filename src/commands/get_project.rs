use super::*;
use crate::utils::config::get_config;
use crate::{sdk::SDK, utils::choice::Choice};
use serde::{Deserialize, Serialize};

/// Get all environment variables for a project
#[derive(Parser)]
pub struct Args {
    /// Partial fingerprint of key to use
    #[clap(short, long)]
    key: Option<String>,

    /// Project ID
    #[clap(short, long)]
    project_id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct User {
    pub id: String,
    pub username: String,
    pub created_at: String, // DateTime
    pub public_key: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ProjectInfo {
    project_id: String,
    users: Vec<User>,
}

pub async fn command(args: Args, _json: bool) -> Result<()> {
    let config = get_config()?;
    let key = config.get_key_or_default(args.key)?;

    let project_id = match args.project_id {
        Some(p) => p,
        None => match config.get_project() {
            Ok(p) => p.project_id.clone(),
            Err(_) => Choice::choose_project(&key.fingerprint).await?,
        },
    };

    let project_info =
        SDK::get_project_info(&project_id, &key.fingerprint, &key.uuid.clone().unwrap()).await?;

    println!("{:?}", project_info);

    Ok(())
}
