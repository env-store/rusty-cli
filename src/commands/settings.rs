use super::*;
use crate::utils::{
    config::{get_config, get_local_config, write_config, Config},
    prompt::prompt_select,
};

/// Set settings in the settings.json.
/// It will break if you edit it manually and do it wrong.
/// Rust hates when the data is not correct :(
#[derive(Parser)]
pub struct Args {
    /// Set globally or locally
    #[clap(short, long)]
    global: bool,

    #[clap(trailing_var_arg = true)]
    args: Vec<String>,
}

pub async fn command(args: Args, _json: bool) -> Result<()> {
    if args.args.len() == 0 {
        println!("No arguments passed, try `envcli settings help`");
        return Ok(());
    };

    let command = match args.args.get(0) {
        Some(s) => s.to_owned(),
        None => {
            return Err(anyhow::anyhow!(
                "No first argument (how did you even do that)"
            ))
        }
    };

    let config = if args.global {
        get_config()?
    } else {
        get_local_config()?
    };

    match command.as_str() {
        "set" => {
            println!("hi");
        }
        "get" => {
            println!("hi");
        }
        "help" => {
            unimplemented!()
        }
        _ => {
            return Err(anyhow::anyhow!(
                "Bad command {}. Try `envcli settings help`",
                command
            ))
        }
    }

    Ok(())
}

fn set(args: Vec<String>) -> Result<()> {
    Ok(())
}
