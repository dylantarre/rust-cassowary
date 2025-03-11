# Rusty Cassowary Music Server

A Rust-based music server designed to run in a Linux container.

## Deployment Instructions

### Portainer Deployment

When deploying to Portainer, follow these steps to avoid common issues:

1. **Environment Variables**: 
   - Set the following environment variables in Portainer:
     - `SUPABASE_JWT_SECRET`: Your JWT secret for authentication
     - `PORT`: 3500 (default)
     - `MUSIC_DIR`: /app/music
     - `RUST_LOG`: info

2. **Volume Mounts**:
   - Mount your music directory to `/app/music` (read-only)
   - **Important**: Do NOT mount the `.env` file directly as this can cause errors

3. **Network**:
   - Ensure the container has proper network access

## Local Development

1. Clone the repository
2. Copy `.env.example` to `.env` and fill in your values
3. Run with `cargo run`

## Docker Compose

For local testing with Docker Compose:

```bash
docker-compose up -d
```

## Requirements

- Rust 1.76+
- Linux audio dependencies (included in Dockerfile):
  - alsa-utils
  - libasound2-dev
  - libpulse0

## Troubleshooting

If you encounter the error "not a directory: unknown: Are you trying to mount a directory onto a file", make sure you're not mounting the `.env` file in Portainer. Instead, set environment variables directly in the Portainer interface.
