//! Stages the Iconify icon sets used by the Topcoat UI components.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    topcoat::icon::iconify::BuildConfig::new()
        .icon_set("lucide")
        .stage()?;
    Ok(())
}
