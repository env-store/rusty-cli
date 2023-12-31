use super::config::{get_config, Config};
use anyhow::anyhow;
use anyhow::{Context, Ok, Result};
use colored::Colorize;
use crypto_hash::{hex_digest, Algorithm};
use hex::ToHex;
use pgp::composed::message::Message;
use pgp::crypto::hash::HashAlgorithm;
use pgp::KeyType;
use pgp::{composed, composed::signed_key::*, crypto, types::SecretKeyTrait, Deserializable};
use rand::prelude::*;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use smallvec::*;
use std::{fs, io::Cursor, path::Path};

#[derive(Debug)]
pub struct KeyPair {
    pub secret_key: pgp::SignedSecretKey,
    pub public_key: pgp::SignedPublicKey,
}

pub(crate) fn get_vault_location() -> anyhow::Result<std::path::PathBuf, anyhow::Error> {
    let path = home::home_dir()
        .context("Failed to get home directory")?
        .join(".config")
        .join("envcli")
        .join("keys");

    Ok(path)
}

#[derive(Clone, Debug)]
pub struct GenerationOptions {
    pub name: String,
    pub email: String,
    pub password: Option<String>,
    pub algorithm: Option<KeyType>,
    pub real_user_id: Option<bool>,
}

impl GenerationOptions {
    pub fn default() -> Self {
        Self {
            name: "".into(),
            email: "".into(),
            password: None,
            algorithm: None,
            real_user_id: None,
        }
    }

    pub fn identity(&mut self, name: &str, email: &str) -> &mut Self {
        self.name = name.into();
        self.email = email.into();
        self
    }

    pub fn password(&mut self, password: &str) -> &mut Self {
        self.password = Some(password.into());
        self
    }

    pub fn algorithm(&mut self, algorithm: KeyType) -> &mut Self {
        self.algorithm = Some(algorithm);
        self
    }

    pub fn use_real_user_id(&mut self) -> &mut Self {
        self.real_user_id = Some(true);
        self
    }

    pub fn build(&mut self) -> Self {
        self.clone()
    }
}

pub fn generate_key_pair(options: GenerationOptions) -> Result<KeyPair, anyhow::Error> {
    let GenerationOptions {
        name,
        email,
        password,
        algorithm,
        real_user_id,
    } = options;

    let mut key_params = composed::key::SecretKeyParamsBuilder::default();

    key_params
        .key_type(match algorithm {
            Some(algo) => algo,
            None => KeyType::Rsa(2048),
        })
        .primary_user_id(match real_user_id {
            Some(true) => format!("{} <{}>", &name, &email),
            _ => generate_hashed_primary_user_id(&name, &email),
        })
        .can_create_certificates(false)
        // .can_sign(true)
        .can_encrypt(true)
        .passphrase(password.clone())
        .preferred_hash_algorithms(smallvec![HashAlgorithm::SHA3_512, HashAlgorithm::SHA2_256])
        .preferred_symmetric_algorithms(smallvec![crypto::sym::SymmetricKeyAlgorithm::AES256]);

    let secret_key_params = key_params
        .build()
        .expect("Must be able to create secret key params");

    let secret_key = secret_key_params
        .generate()
        .expect("Failed to generate a plain key.");

    // rpgp crate requires password to be a FnOnce() -> String
    let passwd_fn = || password.clone().unwrap_or_default();

    let signed_secret_key = secret_key
        .sign(passwd_fn)
        .expect("Secret Key must be able to sign its own metadata");

    let public_key = signed_secret_key.public_key();
    let signed_public_key = public_key
        .sign(&signed_secret_key, passwd_fn)
        .expect("Public key must be able to sign its own metadata");

    let key_pair = KeyPair {
        secret_key: signed_secret_key,
        public_key: signed_public_key,
    };

    Ok(key_pair)
}

pub fn encrypt(msg: &str, pubkey_str: &str) -> Result<String, anyhow::Error> {
    let (pubkey, _) = SignedPublicKey::from_string(pubkey_str)?;
    // Requires a file name as the first arg, in this case I pass "none", as it's not used
    let msg = composed::message::Message::new_literal("none", msg);

    let mut rng = StdRng::from_entropy();
    let new_msg = msg.encrypt_to_keys(
        &mut rng,
        crypto::sym::SymmetricKeyAlgorithm::AES128,
        &[&pubkey],
    )?;
    Ok(new_msg.to_armored_string(None)?)
}

pub fn encrypt_multi(msg: &str, pubkeys: &[SignedPublicKey]) -> Result<String, anyhow::Error> {
    let mut rng = StdRng::from_entropy();

    let borrowed_keys = pubkeys.iter().collect::<SmallVec<[&SignedPublicKey; 1]>>();

    let msg = composed::message::Message::new_literal("none", msg);

    let new_msg = msg.encrypt_to_keys(
        &mut rng,
        crypto::sym::SymmetricKeyAlgorithm::AES128,
        &borrowed_keys,
    )?;

    Ok(new_msg.to_armored_string(None)?)
}

pub fn decrypt(
    armored: &str,
    seckey: &SignedSecretKey,
    password: String,
) -> Result<String, anyhow::Error> {
    let buf = Cursor::new(armored);
    let (msg, _) = composed::message::Message::from_armor_single(buf)
        .context("Failed to convert &str to armored message")?;
    let (mut decryptor, _) = msg
        .decrypt(|| password, &[seckey])
        .context("Decrypting the message")?;

    if let Some(msg) = decryptor.next() {
        let bytes = msg?.get_content()?.unwrap();
        let clear_text = String::from_utf8(bytes)?;
        return Ok(clear_text);
    }

    Err(anyhow::Error::msg("Failed to find message"))
}

pub fn hash_string(input: &str) -> String {
    let hash = hex_digest(Algorithm::SHA512, input.as_bytes());
    hash.to_string()
}

pub fn generate_hashed_primary_user_id(name: &str, email: &str) -> String {
    hash_string(&format!("{}{}{}", name, email, &get_config().unwrap().salt)).to_uppercase()
}

pub fn decrypt_full(message: String, config: &Config) -> Result<String, anyhow::Error> {
    let buf = Cursor::new(message.clone());
    let (msg, _) = composed::message::Message::from_armor_single(buf)
        .context("Failed to convert &str to armored message")?;

    let recipients: Vec<String> = msg
        .get_recipients()
        .iter()
        .map(|e| e.encode_hex_upper())
        .collect();

    let keyring = config
        .keys
        .iter()
        .map(|k| k.fingerprint.clone())
        .collect::<Vec<String>>();

    let available_keys: Vec<String> = keyring
        .iter()
        .filter(|&keyring_key| {
            recipients.iter().any(|recipient_key| {
                keyring_key
                    .to_lowercase()
                    .contains(&recipient_key.to_lowercase())
            })
        })
        .cloned()
        .collect();

    if available_keys.is_empty() {
        return Err(anyhow::anyhow!(
            "{}",
            "No keys available to decrypt this message".red()
        ));
    }

    let primary_key = &config.primary_key;
    let (key, _) = if available_keys.iter().any(|k| k.contains(primary_key)) {
        get_key(primary_key)?
    } else {
        println!("Using key: {}", &available_keys[0]);
        get_key(&available_keys[0])?
    };

    let decrypted = decrypt(message.as_str(), &key, String::from("asdf"))?;

    Ok(decrypted)
}

pub fn decrypt_full_many(
    messages: Vec<String>,
    config: &Config,
) -> Result<Vec<String>, anyhow::Error> {
    let first = messages.first().ok_or_else(|| anyhow!("No messages"))?;
    let msg = Message::from_string(first.as_str())?.0;

    let recipients: Vec<String> = msg
        .get_recipients()
        .iter()
        .map(|e| e.encode_hex_upper())
        .collect();

    let keyring: Vec<String> = config
        .keys
        .iter()
        .map(|k| k.fingerprint.clone())
        .collect::<Vec<String>>();

    let available_keys: Vec<String> = keyring
        .iter()
        .filter(|&keyring_key| {
            recipients.iter().any(|recipient_key| {
                keyring_key
                    .to_lowercase()
                    .contains(&recipient_key.to_lowercase())
            })
        })
        .cloned()
        .collect();

    if available_keys.is_empty() {
        return Err(anyhow::anyhow!(
            "{}",
            "No keys available to decrypt this message".red()
        ));
    }

    let primary_key = &config.primary_key;
    let (key, _) = if available_keys.iter().any(|k| k.contains(primary_key)) {
        get_key(primary_key)?
    } else {
        println!("Using key: {}", &available_keys[0]);
        get_key(&available_keys[0])?
    };

    let decrypted = messages
        .par_iter()
        .map(|m| decrypt(m.as_str(), &key, String::from("asdf")))
        .collect::<Result<Vec<String>, anyhow::Error>>()?;

    Ok(decrypted)
}

fn get_key<T>(fingerprint: T) -> Result<(SignedSecretKey, String)>
where
    T: AsRef<Path> + Into<String>,
{
    let location = get_vault_location()?.join(&fingerprint).join("private.key");

    let priv_key = fs::read_to_string(location).context("Failed to read private key")?;
    let (seckey, _) = SignedSecretKey::from_string(priv_key.as_str())
        .context("Failed to convert private key to string")?;

    Ok((seckey, fingerprint.into()))
}
