use std::process::Command;

fn main() {
    // Check if the container is already running
    let output = Command::new("docker")
        .args(["ps", "-f", "image=postgres"])
        .output()
        .expect("failed to execute process");

    let is_running = output.status.success() && !output.stdout.is_empty();

    if !is_running {
        // Start the Docker container (if not already running)
        // docker run -e POSTGRES_USER=postgres -e POSTGRES_PASSWORD=password -e POSTGRES_DB=ourplaces_db -p 5432:5432 -d postgres postres -N 1000
        Command::new("docker")
            .args([
                "run",
                "-e POSTGRES_USER=postgres",
                "-e",
                "POSTGRES_PASSWORD=password",
                "-e",
                "POSTGRES_DB=ourplaces_db",
                "-p",
                "5432:5432",
                "-d",
                "postgres",
                "-N",
                "1000",
            ])
            .output()
            .expect("failed to execute process");

        // Run sqlx migrations
        // sqlx migrate run
        Command::new("sqlx")
            .args(["migrate", "run"])
            .output()
            .expect("failed to execute process");
    }

    // Proceed with the build
    println!("cargo:rerun-if-changed=build.rs");
}
