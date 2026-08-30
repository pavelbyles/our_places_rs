fn main() {
    topcoat::tailwind::BuildConfig::new()
        .input("src/style/tailwind.css")
        .render()
        .unwrap();
}
