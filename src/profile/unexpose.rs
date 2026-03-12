//! `commands/unexpose.rs`
use anyhow::Result;
use super::store;
use tracing::info;

pub fn handle_unexpose(
    path: Option<String>,
    module: Option<String>,
    domain: Option<String>,
) -> Result<()> {
    let mut profile = store::load_profile()?;

    if let Some(p) = path {
        if profile.custom_paths.remove(&p) {
            info!("Unexposed path: {}", p);
        }
    }

    if let Some(m) = module {
        if profile.modules.remove(&m) {
            info!("Disabled module: {}", m);
        }
    }

    if let Some(d) = domain {
        if profile.allowed_domains.remove(&d) {
            info!("Restricted domain: {}", d);
        }
    }

    store::save_profile(&profile)?;
    Ok(())
}
