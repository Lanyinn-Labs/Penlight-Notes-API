# syntax=docker/dockerfile:1
FROM rust:1.97-alpine AS builder
WORKDIR /app
RUN apk add --no-cache gcc musl-dev
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src ./src
RUN cargo build --release --locked

FROM alpine:3.21
RUN adduser -D -H -u 10001 penlight
COPY --from=builder /app/target/release/penlight-notes-api /usr/local/bin/
USER penlight
EXPOSE 8081
ENV HOST=0.0.0.0 PORT=8081
ENTRYPOINT ["penlight-notes-api"]
