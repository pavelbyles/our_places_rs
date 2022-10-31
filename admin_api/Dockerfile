# Use the official Rust image.
# https://hub.docker.com/_/rust
FROM rust:1.64 as build-env

# Copy local code to the container image.
WORKDIR /usr/src/app
COPY . .

# Install production dependencies and build a release artifact.
#RUN cargo install --path .
RUN cargo build --release

FROM gcr.io/distroless/cc
COPY --from=build-env /usr/src/app/target/release/our-places-app-admin-api-rs /

# Service must listen to $PORT environment variable.
# This default value facilitates local development.
# ENV PORT 8080
# ENV HOST 0.0.0.0
ENV PORT 8080
ENV HOST 0.0.0.0

EXPOSE 8080

# Run the web service on container startup.
CMD ["./our-places-app-admin-api-rs"]