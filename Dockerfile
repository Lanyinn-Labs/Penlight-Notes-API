# syntax=docker/dockerfile:1
FROM rust:1.97-alpine AS builder
WORKDIR /app
RUN apk add --no-cache gcc musl-dev cmake perl
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src ./src
COPY data ./data
COPY vendor ./vendor
RUN cargo build --release --locked

FROM alpine:3.21
RUN adduser -D -H -u 10001 penlight
RUN apk add --no-cache ca-certificates curl
COPY --from=builder /app/target/release/penlight-notes-api /usr/local/bin/
COPY vendor/sirius-api-proxy/protocol /app/vendor/sirius-api-proxy/protocol
COPY data/jp-client.json /app/data/jp-client.json
COPY --chmod=755 scripts/docker-entrypoint.sh /usr/local/bin/penlight-entrypoint
RUN mkdir -p /app/runtime && chown penlight:penlight /app/runtime
COPY LICENSE THIRD-PARTY-NOTICES.md docs/upstream-attribution.md /usr/share/doc/penlight-notes-api/
COPY vendor/sirius-api-proxy/LICENSE /usr/share/doc/penlight-notes-api/SIRIUS-LICENSE
COPY vendor/sirius-api-proxy/LICENSE-protobuf /usr/share/doc/penlight-notes-api/PROTOBUF-LICENSE
WORKDIR /app
USER penlight
EXPOSE 8081
ENV PENLIGHT_LISTEN=0.0.0.0:8081
ENV PENLIGHT_CLIENT_CONFIG=/app/runtime/jp-client.json
ENV PENLIGHT_CLIENT_CONFIG_URL=https://raw.githubusercontent.com/Lanyinn-Labs/Penlight-Notes-API/main/data/jp-client.json
ENTRYPOINT ["penlight-entrypoint"]
