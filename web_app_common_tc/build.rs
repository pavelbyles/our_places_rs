fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=../web_app_tc/src");
    println!("cargo:rerun-if-changed=../web_app_admin_tc/src");
    topcoat::tailwind::BuildConfig::new()
        .input("src/style/tailwind.css")
        .render()?;
    Ok(())
}
