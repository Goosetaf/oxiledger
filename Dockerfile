##########################
#      SERVER BUILD      #
##########################
FROM rust:1.93-alpine AS server-builder

RUN apk add --no-cache \
    build-base \
    musl-dev \
    openssl-dev \
    openssl-libs-static \
    pkgconfig

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo fetch
RUN cargo build --release --no-default-features --features server --offline
RUN rm src/main.rs

COPY src ./src/
COPY assets ./assets/
COPY .sqlx ./.sqlx/
COPY migrations ./migrations/

ENV SQLX_OFFLINE=true
RUN touch src/main.rs
RUN cargo build --release --no-default-features --features server --offline

##########################
#      CLIENT BUILD      #
##########################
FROM rust:1.93-slim-trixie AS client-builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    binaryen \
    ca-certificates \
    curl \
    unzip \
    && rm -rf /var/lib/apt/lists/*

ARG DIOXUS_VERSION=0.7.1
RUN arch="$(uname -m)" && \
    case "$arch" in \
      aarch64) target="aarch64-unknown-linux-gnu" ;; \
      x86_64|amd64) target="x86_64-unknown-linux-gnu" ;; \
      *) echo "Unsupported arch: $arch" >&2; exit 1 ;; \
    esac && \
    curl -fsSL -o /tmp/dx.zip "https://github.com/dioxuslabs/dioxus/releases/download/v${DIOXUS_VERSION}/dx-${target}.zip" && \
    unzip -q /tmp/dx.zip -d /tmp && \
    mv /tmp/dx /usr/local/bin/dx && chmod +x /usr/local/bin/dx && \
    rm -f /tmp/dx.zip && dx --version

RUN rustup target add wasm32-unknown-unknown

WORKDIR /app
COPY Cargo.toml Cargo.lock Dioxus.toml ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo fetch
RUN rm src/main.rs

COPY src ./src/
COPY assets ./assets/
COPY .sqlx ./.sqlx/
COPY migrations ./migrations/

RUN touch src/main.rs
RUN dx build --release --fullstack --force-sequential

##########################
#     ASSET PATCHER      #
##########################
FROM client-builder AS asset-patcher

COPY --from=server-builder /app/target/release/oxiledger /app/oxiledger

RUN mkdir -p /app/patched-assets
RUN dx tools assets /app/oxiledger /app/patched-assets

##########################
#    PRODUCTION STAGE    #
##########################
FROM scratch

WORKDIR /app

COPY --from=server-builder /etc/ssl /etc/ssl
COPY --from=asset-patcher /app/oxiledger ./oxiledger
COPY --from=client-builder /app/target/dx/oxiledger/release/web/public ./public
COPY --from=asset-patcher /app/patched-assets/ ./public/assets/

ENV IP=0.0.0.0
ENV PORT=8080
ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt

ENTRYPOINT ["/app/oxiledger"]
