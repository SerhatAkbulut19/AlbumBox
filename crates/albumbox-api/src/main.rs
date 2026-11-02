//! AlbumBox API.
//!
//! ```text
//! albumbox-api [--config <path>]                 Konsolda çalıştır
//! albumbox-api service install|uninstall|run     Windows servisi
//! albumbox-api reset-password <kullanıcı> <şifre> Şifre sıfırla (kurtarma)
//! ```

mod auth;
mod config;
mod error;
mod lan;
mod limiter;
mod logging;
mod routes;
mod server;
#[cfg(windows)]
mod service;
mod state;

use std::path::PathBuf;

use albumbox_core::{auth as core_auth, users, Db};
use anyhow::{bail, Context};
use config::Config;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut config_path: Option<PathBuf> = None;
    let mut rest = Vec::new();
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        if arg == "--config" {
            config_path = Some(it.next().context("--config requires a path")?.into());
        } else {
            rest.push(arg);
        }
    }

    match rest.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] => run_console(config_path),
        #[cfg(windows)]
        ["service", "install"] => service::install(),
        #[cfg(windows)]
        ["service", "uninstall"] => service::uninstall(),
        #[cfg(windows)]
        ["service", "run"] => service::run_dispatcher(),
        ["reset-password", username, password] => reset_password(config_path, username, password),
        _ => bail!(
            "kullanım: albumbox-api [--config <yol>] | service install|uninstall|run | reset-password <kullanıcı> <şifre>"
        ),
    }
}

fn run_console(config_path: Option<PathBuf>) -> anyhow::Result<()> {
    let config = Config::load(config_path.as_deref())?;
    let _log_guard = logging::init(&config, true);
    tokio::runtime::Runtime::new()?.block_on(server::run(config, async {
        let _ = tokio::signal::ctrl_c().await;
    }))
}

fn reset_password(config_path: Option<PathBuf>, username: &str, password: &str) -> anyhow::Result<()> {
    let config = Config::load(config_path.as_deref())?;
    core_auth::validate_password(password)?;
    tokio::runtime::Runtime::new()?.block_on(async {
        let db = Db::open(&config.db_path()).await?;
        let (user, _) = users::find_for_login(&db, username).await?.context("kullanıcı bulunamadı")?;
        users::set_password_hash(&db, user.id, &core_auth::hash_password(password)?).await?;
        core_auth::delete_user_sessions(&db, user.id).await?;
        println!("'{}' kullanıcısının şifresi değiştirildi.", user.username);
        anyhow::Ok(())
    })
}
