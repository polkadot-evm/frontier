# Docker Setup for Frontier

This directory contains Docker configuration files for running the Frontier template node.

## Quick Start

### Build
```bash
# Build and run the production node
docker compose up --build
```

### Deploy development node
```bash
# Run a development node
docker run -d   --name frontier-node   -p 9933:9933   -p 9944:9944   frontier-node   --dev   --rpc-cors=all   --rpc-external   --rpc-methods=Unsafe   --rpc-port 9933
```

## Files

- `Dockerfile` - Production Docker image for the Frontier template node
- `Dockerfile.dev` - Development Docker image with additional tools
- `docker-compose.yml` - Production Docker Compose configuration
- `docker-compose.dev.yml` - Development Docker Compose configuration with hot reloading
- `.dockerignore` - Files to exclude from Docker build context

## Ports

- `30333` - P2P networking
- `9933` - RPC endpoint
- `9944` - WebSocket endpoint
- `9615` - Prometheus metrics
- `9090` - Prometheus (development only)

## Volumes

- `frontier_data` - Persistent chain data (production)
- `frontier_dev_data` - Development build cache
- `cargo_cache` - Rust cargo cache for faster builds

## Development

The development setup includes:
- Hot reloading of source code
- Debug logging enabled
- Interactive bash shell
- Cargo cache for faster builds
- Optional Prometheus monitoring 