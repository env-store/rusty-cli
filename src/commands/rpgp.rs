use super::*;
use crate::utils::config::get_config;
use crate::utils::prompt::{prompt_confirm, prompt_email, prompt_password, prompt_text};
use crate::utils::rpgp::{get_vault_location, rsa_gen_key, SignedRsaKeyPair};
use anyhow::Context;
use pgp::types::KeyTrait;
use std::fs;

/// Generate a key using rPGP and write it to disk
#[derive(Parser)]
pub struct Args {
    #[clap(short, long)]
    name: Option<String>,

    /// Your email address
    #[clap(short, long)]
    email: Option<String>,

    /// Passphrase to encrypt the key with
    #[clap(short, long)]
    passphrase: Option<String>,
}

fn email_validator(email: &str) -> anyhow::Result<(), anyhow::Error> {
    let regex = regex::Regex::new(r"^[a-zA-Z0-9_.+-]+@[a-zA-Z0-9-]+\.[a-zA-Z0-9-.]+$")
        .context("Failed to create regex for email validation")?;
    if regex.is_match(email) {
        Ok(())
    } else {
        Err(anyhow::Error::msg("Please enter a valid email address"))
    }
}

fn write_key_to_disk(key: SignedRsaKeyPair) -> anyhow::Result<()> {
    let path = rpgp::get_vault_location()?;

    let fingerprint: String = hex::encode(key.public_key.fingerprint());
    let fingerprint = fingerprint.to_uppercase();

    let pub_key_path = path.join(&fingerprint).join("pub.key");
    let priv_key_path = path.join(&fingerprint).join("priv.key");

    fs::create_dir_all(&pub_key_path.parent().unwrap())?;

    fs::write(&pub_key_path, key.public_key())?;
    fs::write(&priv_key_path, key.secret_key())?;

    Ok(())
}

pub async fn command(args: Args, _json: bool) -> Result<()> {
    let name = args
        .name
        .unwrap_or_else(|| prompt_text("What is your name?").unwrap());
    let email = args.email.unwrap_or_else(|| prompt_email("email").unwrap());

    match email_validator(&email) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }

    let passphrase = args
        .passphrase
        .unwrap_or_else(|| prompt_password("password").unwrap());

    let start = std::time::Instant::now();
    let key = rsa_gen_key(
        &name,
        "(env-store encryption key generated by env-cli)",
        &email,
        &passphrase,
    )
    .context("Failed to generate key")?;
    println!("Generated key in {}s", start.elapsed().as_secs());

    write_key_to_disk(key.clone())?;

    let fingerprint = key.fingerprint();

    let pub_key_path = home::home_dir()
        .unwrap()
        .join(".config")
        .join("envcli")
        .join("keys")
        .join(&fingerprint);

    let config = get_config()?;
    let mut keys = config.keys.clone();
    keys.push(fingerprint.clone());

    println!("Generated key with fingerprint {}", fingerprint.red());
    println!("Public key saved to {}", pub_key_path.display());

    if config.primary_key == "" {
        println!("This is now your primary key");
    } else {
        match prompt_confirm("Would you like to make this your primary key?")? {
            true => {
                let new_config = crate::utils::config::Config {
                    user_id: config.user_id,
                    password: config.password,
                    primary_key: fingerprint.clone(),
                    keys: keys,
                };

                crate::utils::config::write_config(&new_config)?;

                println!("{} is now your primary key", fingerprint.red());
            }
            false => {
                let new_config = crate::utils::config::Config {
                    user_id: config.user_id,
                    password: config.password,
                    primary_key: config.primary_key.clone(),
                    keys: keys,
                };

                crate::utils::config::write_config(&new_config)?;
                println!("Your primary key was unchanged");
                println!("{} is your primary key", config.primary_key.red());
            }
        }
    }

    Ok(())
}
