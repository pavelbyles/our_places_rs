use anyhow::{Context, Result, bail};
use sqlx::postgres::PgPoolOptions;
use std::env;
use std::path::{Path, PathBuf};

/// Default embedded admin user seed script.
/// Ensures the binary is completely hermetic and self-contained even when executed
/// without access to the source repository.
const EMBEDDED_ADMIN_SEED: &str = include_str!("../../seeds/001_admin_user.sql");

#[derive(Debug)]
struct CliConfig {
    database_url: Option<String>,
    custom_path: Option<PathBuf>,
}

fn parse_args() -> Result<Option<CliConfig>> {
    let args: Vec<String> = env::args().collect();
    let mut database_url = None;
    let mut custom_path = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                println!("Our Places - Hermetic Database Seeder");
                println!();
                println!("USAGE:");
                println!("    seed-db [OPTIONS]");
                println!();
                println!("OPTIONS:");
                println!(
                    "    --path <PATH>            Custom SQL file or directory of SQL files to execute"
                );
                println!(
                    "    --database-url <URL>     PostgreSQL connection URI (defaults to DATABASE_URL in .env)"
                );
                println!("    -h, --help               Print help information");
                return Ok(None);
            }
            "--database-url" => {
                i += 1;
                let val = args.get(i).context("Missing value for --database-url")?;
                database_url = Some(val.clone());
            }
            "--path" => {
                i += 1;
                let val = args.get(i).context("Missing value for --path")?;
                custom_path = Some(PathBuf::from(val));
            }
            arg if arg.starts_with("--database-url=") => {
                let val = arg.strip_prefix("--database-url=").unwrap_or_default();
                database_url = Some(val.to_string());
            }
            arg if arg.starts_with("--path=") => {
                let val = arg.strip_prefix("--path=").unwrap_or_default();
                custom_path = Some(PathBuf::from(val));
            }
            other => {
                bail!("Unknown argument: {other}. Use --help for usage information.");
            }
        }
        i += 1;
    }

    Ok(Some(CliConfig {
        database_url,
        custom_path,
    }))
}

struct SeedScript {
    name: String,
    content: String,
}

fn collect_seed_scripts(custom_path: Option<&Path>) -> Result<Vec<SeedScript>> {
    if let Some(path) = custom_path {
        if !path.exists() {
            bail!("Specified path does not exist: {}", path.display());
        }

        if path.is_file() {
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "custom_script.sql".to_string());
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read SQL script from {}", path.display()))?;
            return Ok(vec![SeedScript { name, content }]);
        }

        if path.is_dir() {
            let mut entries = Vec::new();
            for entry in std::fs::read_dir(path)
                .with_context(|| format!("Failed to read directory: {}", path.display()))?
            {
                let entry = entry?;
                let file_path = entry.path();
                if file_path.is_file()
                    && file_path.extension().and_then(|ext| ext.to_str()) == Some("sql")
                {
                    entries.push(file_path);
                }
            }
            entries.sort();

            if entries.is_empty() {
                bail!("No .sql files found in directory: {}", path.display());
            }

            let mut scripts = Vec::new();
            for p in entries {
                let name = p
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "seed.sql".to_string());
                let content = std::fs::read_to_string(&p)
                    .with_context(|| format!("Failed to read SQL script from {}", p.display()))?;
                scripts.push(SeedScript { name, content });
            }
            return Ok(scripts);
        }
    }

    // Default discovery: Check db_core/seeds directory relative to current or ancestor dirs
    let relative_seeds_candidates = [
        PathBuf::from("db_core/seeds"),
        PathBuf::from("seeds"),
        PathBuf::from("../seeds"),
    ];

    for candidate in &relative_seeds_candidates {
        if let Ok(entries) = std::fs::read_dir(candidate) {
            let mut sql_paths = Vec::new();
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("sql") {
                    sql_paths.push(p);
                }
            }
            sql_paths.sort();

            if !sql_paths.is_empty() {
                let mut scripts = Vec::new();
                for p in sql_paths {
                    let name = p
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "seed.sql".to_string());
                    let content = std::fs::read_to_string(&p).with_context(|| {
                        format!("Failed to read SQL script from {}", p.display())
                    })?;
                    scripts.push(SeedScript { name, content });
                }
                return Ok(scripts);
            }
        }
    }

    // Hermetic fallback: Use compile-time embedded script
    Ok(vec![SeedScript {
        name: "001_admin_user.sql (embedded)".to_string(),
        content: EMBEDDED_ADMIN_SEED.to_string(),
    }])
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let config = match parse_args()? {
        Some(c) => c,
        None => return Ok(()),
    };

    let database_url = config
        .database_url
        .or_else(|| env::var("DATABASE_URL").ok())
        .context("DATABASE_URL is not set. Specify via --database-url or in .env file.")?;

    let scripts = collect_seed_scripts(config.custom_path.as_deref())?;

    println!("============================================================");
    println!("🌱 Our Places - Hermetic Database Seeder");
    println!("============================================================");

    // Sanitize database URL for logging (mask password if present)
    let sanitized_url = if let Some((prefix, rest)) = database_url.split_once('@') {
        if let Some((proto, user_pass)) = prefix.split_once("://") {
            if let Some((user, _pass)) = user_pass.split_once(':') {
                format!("{proto}://{user}:*****@{rest}")
            } else {
                format!("{proto}://*****@{rest}")
            }
        } else {
            format!("*****@{rest}")
        }
    } else {
        database_url.clone()
    };
    println!("🔌 Target Database: {}", sanitized_url);
    println!("📦 Seed scripts to apply: {}", scripts.len());
    println!("------------------------------------------------------------");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .with_context(|| format!("Failed to connect to database at {sanitized_url}"))?;

    for script in scripts {
        println!("--> Applying script: {}...", script.name);

        let mut tx = pool
            .begin()
            .await
            .with_context(|| format!("Failed to start transaction for {}", script.name))?;

        sqlx::raw_sql(&script.content)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("Failed to execute SQL in {}", script.name))?;

        tx.commit()
            .await
            .with_context(|| format!("Failed to commit transaction for {}", script.name))?;

        println!("    ✓ Successfully applied {}", script.name);
    }

    println!("------------------------------------------------------------");
    println!("✅ Database seeding completed successfully!");
    println!();
    println!("🔑 Initial Admin Login Credentials:");
    println!("   • Email:    admin@ourplaces.io");
    println!("   • Password: admin_changeme_2026");
    println!("   • Roles:    [admin, host]");
    println!("============================================================");

    Ok(())
}
