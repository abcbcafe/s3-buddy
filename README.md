# s3-buddy

A Rust service that automatically maintains short, persistent URLs for S3 objects using Route53 and presigned URLs.

## Features

- Takes an S3 URL (s3://bucket/key)
- Generates presigned URLs with 12-hour duration
- Creates/updates a static short URL in Route53 that points to the presigned URL
- Automatically refreshes the presigned URL every 11 hours (before expiration)
- Assumes AWS credentials exist at ~/.aws/credentials

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

## Usage

```bash
s3-buddy <s3-url> <short-url> <hosted-zone-id>
```

### Example

```bash
s3-buddy s3://my-bucket/path/to/file.pdf short.example.com Z1234567890ABC
```

This will:
1. Generate a presigned URL for `s3://my-bucket/path/to/file.pdf`
2. Create/update a CNAME record for `short.example.com` pointing to the presigned URL
3. Automatically refresh the URL every 11 hours

## Configuration

The service uses the following defaults:
- Presigned URL duration: 12 hours
- Refresh interval: 11 hours (1 hour before expiration)
- DNS TTL: 5 minutes

## Logging

Set the `RUST_LOG` environment variable to control logging level:

```bash
RUST_LOG=debug s3-buddy s3://bucket/key short.example.com Z123
```

## Testing

```bash
cargo test
```

## Architecture

- **config**: Configuration management and validation
- **s3**: S3 presigned URL generation using AWS SDK
- **route53**: Route53 DNS record management (CNAME updates)
- **scheduler**: Tokio-based periodic refresh mechanism

## License

Licensed under the GPLv3
