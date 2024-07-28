# Use the official Rust image.
# https://hub.docker.com/_/rust
# syntax=docker/dockerfile:1
ARG BUILD_TYPE=release
ARG BUILD_OUTPUT=release
FROM rust:1.80.0 as build-env
ARG BUILD_TYPE
ARG BUILD_OUTPUT
ARG POSTGRES_USER
ARG POSTGRES_PASSWORD
ARG POSTGRES_HOST
ARG POSTGRES_DB
RUN echo "Build type is: ${BUILD_TYPE}"
RUN echo "Build output is: ${BUILD_OUTPUT}"

# Copy local code to the container image.
WORKDIR /usr/src/app
COPY . .

# Install production dependencies and build a release artifact.
# RUN cargo install --path .
RUN cargo install cargo-make
RUN DATABASE_URL=postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@${POSTGRES_HOST}:5432/${POSTGRES_DB}
RUN cargo make ${BUILD_TYPE}-build-flow

FROM gcr.io/distroless/cc
ARG BUILD_OUTPUT
COPY --from=build-env /usr/src/app/target/out/our-places-app-api-rs/${BUILD_OUTPUT} /
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
