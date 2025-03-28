FROM rust:1.84.0-alpine AS builder

RUN apk add --no-cache musl-dev cmake make linux-headers gcc g++ protoc upx

WORKDIR /build

COPY . .
RUN cargo build --release
RUN upx -9 target/release/pando-rs

FROM alpine

RUN apk add \
    ca-certificates

COPY --from=builder /build/target/release/pando-rs /usr/bin/pando-agent

CMD ["/usr/bin/pando-agent"]
