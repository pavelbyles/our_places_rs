use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: topcoat_bundler <binary_path> <out_dir> [target_dir]");
        std::process::exit(1);
    }
    let binary_path = PathBuf::from(&args[1]);
    let out_dir = PathBuf::from(&args[2]);
    let target_dir = if args.len() > 3 {
        PathBuf::from(&args[3])
    } else {
        binary_path
            .parent()
            .and_then(|p| p.parent())
            .unwrap_or_else(|| Path::new("target"))
            .to_path_buf()
    };
    let cache_dir = target_dir.join("topcoat").join("cache").join("assets");
    std::fs::create_dir_all(&cache_dir).ok();
    println!("Using cache_dir: {}", cache_dir.display());

    let bytes = std::fs::read(&binary_path)?;
    let config = topcoat_asset::BundlerConfig::new().cache_dir(cache_dir);
    topcoat_asset::Bundler::new(&config).bundle(&bytes, &out_dir)?;
    println!("Successfully bundled assets to {}", out_dir.display());
    Ok(())
}
