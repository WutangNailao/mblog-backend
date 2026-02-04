FROM rust:1.93-alpine AS builder
WORKDIR /app

RUN apk add --no-cache build-base pkgconf openssl-dev openssl-libs-static ca-certificates

RUN rustup target add x86_64-unknown-linux-musl

COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && echo "fn main(){}" > src/main.rs
RUN cargo build -r --target x86_64-unknown-linux-musl

RUN rm -rf ./src
COPY . ./
RUN touch ./src/main.rs
RUN cargo build -r --target x86_64-unknown-linux-musl

FROM scratch
WORKDIR /app

COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/mblog-backend-rs /app/mblog-backend-rs
COPY --from=builder /app/changelog-sqlite.sql /app/changelog-sqlite.sql

ENV RUST_LOG=info
ENV SQLITE_PATH=/data/mblog.sqlite
ENV SERVER_PORT=38322

VOLUME ["/data"]
EXPOSE 38322

CMD ["/app/mblog-backend-rs"]
