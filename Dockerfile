FROM rust:1.79 as builder

WORKDIR /build
COPY . .
RUN cargo build --release --locked -p tokfin-node

FROM debian:bullseye-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/tokfin-node /usr/local/bin/tokfin-node

EXPOSE 30333 9933 9944
ENTRYPOINT ["tokfin-node"]
