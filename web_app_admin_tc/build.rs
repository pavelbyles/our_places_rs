fn main() -> Result<(), Box<dyn std::error::Error>> {
    topcoat::tailwind::BuildConfig::new()
        .input("src/style/tailwind.css")
        .render()?;
    Ok(())
}
