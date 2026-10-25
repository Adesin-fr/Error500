# syntax=docker/dockerfile:1

# --- Frontend : Vue 3 + Inertia, construit par Vite (le résultat est embarqué dans le binaire Rust) ---
FROM node:22-bookworm-slim AS frontend
WORKDIR /app/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

# --- Backend : les dépendances d'abord (couche mise en cache tant que Cargo.toml/Cargo.lock ne changent pas) ---
FROM rust:1-bookworm AS backend
WORKDIR /app
COPY Cargo.toml Cargo.lock build.rs ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && touch src/lib.rs \
    && cargo build --release \
    && rm -rf src target/release/error500* target/release/deps/error500* target/release/.fingerprint/error500*
COPY migrations ./migrations
COPY src ./src
COPY --from=frontend /app/frontend/dist ./frontend/dist
RUN cargo build --release

# --- Image finale ---
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home-dir /data --shell /usr/sbin/nologin error500 \
    && mkdir -p /data/blobs \
    && chown -R error500 /data

COPY --from=backend /app/target/release/error500 /usr/local/bin/error500

ENV BIND=0.0.0.0:8080 \
    STORAGE=local \
    STORAGE_PATH=/data/blobs
USER error500
WORKDIR /data
VOLUME /data
EXPOSE 8080
ENTRYPOINT ["error500"]
