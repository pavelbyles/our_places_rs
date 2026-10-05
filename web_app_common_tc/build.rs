fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=src");
    topcoat::tailwind::BuildConfig::new()
        .input("src/style/tailwind.css")
        .render()?;
    Ok(())
}
