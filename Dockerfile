FROM rust:1.75-slim

# Install build dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    clang \
    libclang-dev \
    libssl-dev \
    pkg-config \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy source code from frontier directory
COPY . .

# Build the node
RUN cd template/node && cargo build --release --bin frontier-template-node

# Expose ports
EXPOSE 30333 9933 9944 9615

# Run the node
ENTRYPOINT ["/app/template/node/target/release/frontier-template-node"]
CMD ["--dev", "--rpc-cors=all", "--rpc-external", "--rpc-methods=Unsafe", "--rpc-port=9933"] 
