# syntax=docker/dockerfile:1
# A team's server: the release binary, carrying the interface, serving a library.
# Built with Docker alone: `docker build -t grimoire .`. How to run, secure, back
# up and upgrade it is on the grimoire's page Running a team's server.

# The interface, built with Node.
FROM node:24-bookworm-slim AS web
WORKDIR /src/web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

# The release binary, built with Rust, with the interface embedded. The registry
# and the target are cache mounts, so a rebuild compiles only what changed.
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
COPY --from=web /src/web/dist web/dist
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p grimoire-cli \
    && cp target/release/grimoire /usr/local/bin/grimoire

# The image: the binary alone, run by a user of its own who owns the library.
# The uid is fixed, so a named volume is created writable by it and every
# container on it, a fast path's included, writes as the same user.
FROM debian:bookworm-slim
RUN groupadd --system --gid 10001 grimoire \
    && useradd --system --uid 10001 --gid grimoire --no-create-home grimoire \
    && mkdir /data && chown grimoire:grimoire /data
COPY --from=build /usr/local/bin/grimoire /usr/local/bin/grimoire
USER grimoire
VOLUME /data
EXPOSE 4243
CMD ["grimoire", "serve", "--shared", "--library", "/data", "--listen", "0.0.0.0:4243"]
