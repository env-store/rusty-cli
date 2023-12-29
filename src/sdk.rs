use super::*;
use crate::{
    types::ProjectInfo,
    utils::{
        auth::get_token,
        config::get_config,
        kvpair::KVPair,
        rpgp::{decrypt_full_many, encrypt_multi},
    },
};
use anyhow::Ok;
use reqwest::header;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Serialize, Deserialize, Debug)]
pub struct NewUserParams {
    pub fingerprint: String,
    pub user_id: String,
    pub pubkey: String,
    pub pubkey_hash: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SetEnvParams {
    pub message: String,
    pub allowed_keys: Vec<String>,
    pub project_id: Option<String>,
}

pub fn get_api_url() -> Result<String> {
    Ok(get_config()?
        .sdk_url
        .unwrap_or("http://localhost:3000".into()))
}

#[allow(clippy::upper_case_acronyms)]
pub(crate) struct SDK {}
impl SDK {
    pub async fn new_user_old(user: &NewUserParams) -> Result<()> {
        let client = reqwest::Client::new();

        let res = client
            .post(&format!("{}/user/new", get_api_url()?))
            .json(&user)
            .send()
            .await?;

        let status = res.status();

        if status.is_success() {
            Ok(())
        } else {
            Err(anyhow!(format!(
                "Failed to create user: {}",
                res.text().await?
            )))
        }
    }

    pub async fn set_env_old(body: SetEnvParams) -> Result<()> {
        let client = reqwest::Client::new();

        let project_id = match body.project_id {
            Some(pid) => pid,
            None => "null".into(),
        };

        let body = json!({
            "message": body.message,
            "allowed_keys": body.allowed_keys,
            "project_id": project_id
        });

        let res = client
            .post(&format!("{}/secrets/new", get_api_url()?))
            .json(&body)
            .send()
            .await?;

        let status = res.status();

        if status.is_success() {
            Ok(())
        } else {
            Err(anyhow!(format!(
                "Failed to set new secret: {}",
                "err" // res.text().await?
            )))
        }
    }

    pub async fn get_project_info(
        project_id: &str,
        partial_fingerprint: &str,
        user_id: &str,
    ) -> Result<ProjectInfo> {
        let client = reqwest::Client::new();

        let auth_token = get_token(&partial_fingerprint, &user_id).await?;

        let project_info = client
            .get(&format!("{}/project/{}", get_api_url()?, project_id))
            .header(header::AUTHORIZATION, format!("Bearer {}", auth_token))
            .send()
            .await?
            .json::<ProjectInfo>()
            .await?;

        Ok(project_info)
    }

    pub async fn set_env(
        key: &str,
        value: &str,
        partial_fingerprint: &str,
        project_id: &str,
        user_id: &str,
    ) -> Result<String> {
        let client = reqwest::Client::new();
        let auth_token = get_token(&partial_fingerprint, &user_id).await?;

        let project_info = Self::get_project_info(project_id, partial_fingerprint, user_id).await?;

        let recipients = project_info
            .users
            .iter()
            .map(|u| u.public_key.as_str())
            .collect::<Vec<&str>>();

        let kvpair = json!({
            "key": key,
            "value": value
        })
        .to_string();
        let message = encrypt_multi(&kvpair, recipients)?;

        let body = json!({
            "project_id": project_id,
            "value": message,
        });

        #[derive(Serialize, Deserialize, Debug)]
        pub struct NewVariableReturnType {
            pub id: String,
        }

        let res = client
            .post(&format!("{}/variable/new", get_api_url()?))
            .header(header::AUTHORIZATION, format!("Bearer {}", auth_token))
            .json(&body)
            .send()
            .await?;

        let res = res.json::<NewVariableReturnType>().await?;

        dbg!("Got here 2");

        Ok(res.id)
    }

    pub async fn set_many(
        kvpairs: Vec<KVPair>,
        partial_fingerprint: &str,
        project_id: &str,
        user_id: &str,
    ) -> Result<Vec<String>> {
        let client = reqwest::Client::new();
        let auth_token = get_token(&partial_fingerprint, &user_id).await?;

        let project_info = Self::get_project_info(project_id, partial_fingerprint, user_id).await?;

        let recipients = project_info
            .users
            .iter()
            .map(|u| u.public_key.as_str())
            .collect::<Vec<&str>>();

        let messages = kvpairs
            .iter()
            .map(|k| encrypt_multi(&k.to_json().unwrap(), recipients.clone()).unwrap())
            .collect::<Vec<String>>();

        let body = json!({
            "project_id": project_id,
            "variables": messages,
        });

        #[derive(Serialize, Deserialize, Debug)]
        pub struct SetManyVariableReturnType {
            pub id: String,
        }

        let res = client
            .post(&format!("{}/variables/set-many", get_api_url()?))
            .header(header::AUTHORIZATION, format!("Bearer {}", auth_token))
            .json(&body)
            .send()
            .await?;

        let res = res.json::<Vec<SetManyVariableReturnType>>().await?;

        Ok(res.iter().map(|r| r.id.clone()).collect())
    }

    pub async fn get_variables(
        project_id: &str,
        partial_fingerprint: &str,
        user_id: &str,
    ) -> Result<Vec<KVPair>> {
        // url : /project/:id/variables
        let client = reqwest::Client::new();
        let auth_token = get_token(&partial_fingerprint, &user_id).await?;

        let encrypted = client
            .get(&format!(
                "{}/project/{}/variables",
                get_api_url()?,
                project_id
            ))
            .header(header::AUTHORIZATION, format!("Bearer {}", auth_token))
            .send()
            .await?
            .json::<Vec<String>>()
            .await?;

        let decrypted = decrypt_full_many(encrypted, &get_config().unwrap())?;

        let parsed = decrypted
            .iter()
            .map(|d| KVPair::from_json(d.clone()).unwrap())
            .collect::<Vec<KVPair>>();

        Ok(parsed)
    }
}
