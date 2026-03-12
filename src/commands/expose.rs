//! `commands/expose.rs`
use anyhow::Result;
use crate::profile::{store, validator};
use tracing::info;

pub fn handle_expose(
    path: Option<String>,
    module: Option<String>,
    domain: Option<String>,
) -> Result<()> {
    let mut profile = store::load_profile()?;

    if let Some(p) = path {
        validator::validate_custom_path(&p)?;
        profile.custom_paths.insert(p.clone());
        info!("Exposed path: {}", p);
    }

    if let Some(m) = module {
        profile.modules.insert(m.clone());
        info!("Enabled module: {}", m);
    }

    if let Some(d) = domain {
        profile.allowed_domains.insert(d.clone());
        info!("Allowed domain: {}", d);
    }

    store::save_profile(&profile)?;
    Ok(())
}
