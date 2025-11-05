# s3-buddy

A Rust service that automatically maintains short, persistent URLs for S3 objects using Route53 and presigned URLs, featuring a client/server architecture with an interactive TUI.

## Features

- **Client/Server Architecture**: Manage multiple URL mappings from a centralized server
- **Interactive TUI**: Beautiful terminal interface for managing mappings
- **Automatic Refresh**: Generates presigned URLs with configurable duration
- **Multiple Mappings**: Track and manage multiple S3 URL mappings simultaneously
- **Real-time Status**: Monitor the health and status of each mapping
- **Pause/Resume**: Control refresh operations for individual mappings

## Architecture

s3-buddy now features a client/server architecture:

### Server (`s3-buddy-server`)
- HTTP REST API for managing URL mappings
- Manages multiple S3 URL mappings concurrently
- Each mapping runs its own refresh scheduler
- Automatic presigned URL refresh before expiration
- Status tracking and error reporting

### Client (`s3-buddy-client`)
- Interactive TUI built with Ratatui
- Real-time dashboard showing all mappings
- Add, edit, delete, pause, and resume mappings
- Keyboard-driven interface for efficient management

### Legacy CLI (`s3-buddy`)
- Original single-mapping command-line interface
- Still available for simple use cases

## Prerequisites

- Rust 1.70 or later
- AWS credentials configured in ~/.aws/credentials
- AWS permissions for:
  - S3: `s3:GetObject` on the target bucket/object
  - Route53: `route53:ChangeResourceRecordSets` on the hosted zone

## Installation

```bash
cargo build --release
```

This builds three binaries:
- `target/release/s3-buddy-server` - The server
- `target/release/s3-buddy-client` - The TUI client
- `target/release/s3-buddy` - Legacy CLI

## Usage

### Client/Server Mode (Recommended)

#### 1. Start the Server

```bash
# Default port 3000
./target/release/s3-buddy-server

# Custom port
PORT=8080 ./target/release/s3-buddy-server
```

#### 2. Start the TUI Client

```bash
# Connect to local server (default)
./target/release/s3-buddy-client

# Connect to remote server
S3_BUDDY_SERVER=http://your-server:3000 ./target/release/s3-buddy-client
```

#### 3. Using the TUI

**Dashboard View:**
- `↑/↓` or `j/k` - Navigate mappings
- `a` - Add new mapping
- `e` - Edit selected mapping
- `d` - Delete selected mapping
- `p` - Pause/Resume selected mapping
- `r` - Refresh mappings list
- `?` - Show help
- `q` - Quit

**Form View:**
- `Tab` - Next field
- `Shift+Tab` - Previous field
- `Enter` - Submit
- `Esc` - Cancel

### Legacy CLI Mode

```bash
s3-buddy <s3-url> <short-url> <hosted-zone-id>
```

Example:
```bash
s3-buddy s3://my-bucket/path/to/file.pdf short.example.com Z1234567890ABC
```

## API Endpoints

The server exposes the following REST API:

- `GET /health` - Health check
- `GET /mappings` - List all mappings
- `POST /mappings` - Create a new mapping
- `GET /mappings/:id` - Get a specific mapping
- `PUT /mappings/:id` - Update a mapping
- `DELETE /mappings/:id` - Delete a mapping
- `POST /mappings/:id/pause` - Pause a mapping
- `POST /mappings/:id/resume` - Resume a mapping

### Example API Usage

```bash
# Create a mapping
curl -X POST http://localhost:3000/mappings \
  -H "Content-Type: application/json" \
  -d '{
    "s3_url": "s3://my-bucket/file.pdf",
    "short_url": "short.example.com",
    "hosted_zone_id": "Z1234567890ABC",
    "presign_duration_secs": 43200,
    "refresh_interval_secs": 39600
  }'

# List all mappings
curl http://localhost:3000/mappings

# Pause a mapping
curl -X POST http://localhost:3000/mappings/{id}/pause
```

## Configuration

Each mapping supports:
- **Presigned URL duration**: Default 12 hours (configurable)
- **Refresh interval**: Default 11 hours (configurable)
- **DNS TTL**: 5 minutes

## Logging

Set the `RUST_LOG` environment variable to control logging level:

```bash
RUST_LOG=debug ./target/release/s3-buddy-server
```

## Testing

```bash
cargo test
```

## Module Architecture

- **config**: Configuration management and validation
- **s3**: S3 presigned URL generation using AWS SDK
- **route53**: Route53 DNS record management (CNAME updates)
- **scheduler**: Tokio-based periodic refresh mechanism (legacy)
- **manager**: Multi-mapping management and orchestration
- **server**: HTTP REST API server
- **tui**: Terminal user interface client
- **types**: Shared data structures

## License

Licensed under the GPLv3
