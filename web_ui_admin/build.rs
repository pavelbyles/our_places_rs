//! Compiles the Tailwind stylesheet for the portal.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Tailwind scans this crate and web_ui_common for class names (see
    // src/style/tailwind.css), so a change in either must rebuild the CSS.
    // Cargo watches directories recursively.
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=../web_ui_common/src");
    topcoat::tailwind::BuildConfig::new()
        .input("src/style/tailwind.css")
        .render()?;
    Ok(())
}
