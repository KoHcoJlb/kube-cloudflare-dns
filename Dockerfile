FROM rust:alpine3.14 AS build

ENV RUSTFLAGS="-C target-feature=-crt-static"

RUN apk add musl-dev openssl-dev

COPY Cargo.toml Cargo.lock /build/
COPY src/bin/ /build/src/bin/

RUN cd build && cargo build --release --bin dummy

COPY . /build/
RUN cd build && cargo build --release

FROM alpine:3.14

RUN apk add libgcc
COPY --from=build /build/target/release/kube-cloudflare-dns /usr/bin/

ENTRYPOINT /usr/bin/kube-cloudflare-dns
