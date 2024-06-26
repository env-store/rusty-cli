use super::*;
use crate::{
    sdk::get_api_url,
    utils::{auth::get_token, config::get_config},
};
use anyhow::bail;
use reqwest::header;

/// Test authentication with the server
#[derive(Parser)]
pub struct Args {
    /// Key to sign with
    #[clap(short, long)]
    key: Option<String>,
}

pub async fn command(args: Args) -> anyhow::Result<()> {
    let config = get_config()?;
    let key = config.get_key_or_default(args.key)?;

    let client = reqwest::Client::new();

    let uuid = key
        .uuid
        .clone()
        .context("Key does not have a UUID, try `envx upload`")?;
    let auth_token = get_token(&key.fingerprint, &uuid)
        .await
        .context("Failed to get token")?;

    println!("auth token:\n{}", auth_token.signature);

    let res = client
        .post(format!("{}/test-auth", get_api_url()))
        .header(header::AUTHORIZATION, format!("Bearer {}", auth_token))
        .send()
        .await?;

    let status = res.status();

    if status.is_success() {
        println!("success");
        // print the text response

        let text = res.text().await?;

        println!("{}", text);
    } else {
        println!("status: {}", status);
        bail!("failed to auth")
    }

    Ok(())
}
