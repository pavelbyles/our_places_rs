#!/usr/bin/env bash
rustfmt | leptosfmt --stdin

# | rustywind --output-css-file "$(pwd)/web_app/style/main.scss" --stdin
