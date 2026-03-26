use leptos::config::get_configuration;

fn main() {
    let conf = get_configuration(None).unwrap();
    println!("{:?}", conf.leptos_options.site_addr);
}
