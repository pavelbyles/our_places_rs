# Use the official Rust image.
# https://hub.docker.com/_/rust
# syntax=docker/dockerfile:1
ARG BUILD_TYPE=release
FROM rust:1.68 as build-env

# Copy local code to the container image.
WORKDIR /usr/src/app
COPY . .

# Install production dependencies and build a release artifact.
# RUN cargo install --path .
RUN cargo install cargo-make
RUN cargo make ${BUILD-TYPE}-build-flow

FROM gcr.io/distroless/cc
COPY --from=build-env /usr/src/app/target/release/our-places-app-api-rs /
COPY config config
COPY log4rs.yml log4rs.yml

# Service must listen to $PORT environment variable.
# This default value facilitates local development.
# ENV PORT 8080
# ENV HOST 0.0.0.0
# ENV RUN_ENV Development

# EXPOSE 8080

# Run the web service on container startup.
CMD ["./our-places-app-api-rs"]
