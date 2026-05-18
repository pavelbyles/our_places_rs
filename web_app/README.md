# web_app

`web_app` is the primary customer-facing frontend for the `our_places_rs` villa booking system. It is built using the [Leptos](https://github.com/leptos-rs/leptos) web framework, compiled to WebAssembly (Wasm).

## Running your project

`cargo leptos watch`  
By default, you can access your local project at `http://localhost:3000`

## Executing a Server on a Remote Machine Without the Toolchain
After running a `cargo leptos build --release` the minimum files needed are:

1. The server binary located in `target/server/release`
2. The `site` directory and all files within located in `target/site`

Copy these files to your remote server. The directory structure should be:
```text
leptos_start
site/
```
Set the following environment variables (updating for your project as needed):
```sh
export LEPTOS_OUTPUT_NAME="leptos_start"
export LEPTOS_SITE_ROOT="site"
export LEPTOS_SITE_PKG_DIR="pkg"
export LEPTOS_SITE_ADDR="127.0.0.1:3000"
export LEPTOS_RELOAD_PORT="3001"
```
Finally, run the server binary.
