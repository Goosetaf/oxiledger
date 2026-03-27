FROM rust:1.94-alpine AS builder

RUN apk add --no-cache musl-dev build-base pkgconfig openssl-dev openssl-libs-static ca-certificates binaryen

RUN cargo install cargo-binstall
RUN cargo binstall dioxus-cli@0.7.1

RUN rustup target add wasm32-unknown-unknown

WORKDIR /app
COPY Cargo.toml Cargo.lock Dioxus.toml ./

RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo fetch
RUN cargo build --release
RUN rm src/main.rs

COPY src ./src/
COPY assets ./assets/

RUN touch src/main.rs

RUN dx build --release --verbose

##########################
#    PRODUCTION STAGE    #
##########################
FROM scratch

WORKDIR /app

COPY --from=builder /etc/ssl /etc/ssl
COPY --from=builder /app/target/dx/oxiledger/release/web/oxiledger ./oxiledger

COPY --from=builder /app/target/dx/oxiledger/release/web/public ./public

ENV IP=0.0.0.0
ENV PORT=8080
ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt

ENTRYPOINT ["/app/oxiledger"]
