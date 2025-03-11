# Rusty Cassowary Music Server

A Rust-based music server designed to run in a Linux container with Supabase authentication integration.

## Features

- Streaming MP3 audio files with range request support
- JWT-based authentication via Supabase
- Prefetching mechanism for improved playback performance
- Containerized for Kubernetes/Portainer deployment
- CORS support for web clients
- Compatible with music-cli application

## Authentication

This server uses Supabase for authentication. It verifies JWT tokens issued by Supabase to authenticate API requests.

### Setting Up Supabase

1. Create a Supabase project at [https://supabase.com](https://supabase.com)
2. Enable email authentication in your Supabase project settings
3. Get your JWT secret from Supabase project settings
4. Set the `SUPABASE_JWT_SECRET` environment variable in your deployment

### Authentication Integration

This service can be integrated with various client applications:

- CLI tools that handle Supabase authentication
- Mobile applications
- Web applications
- Other services that can obtain and use Supabase JWT tokens

Any client that can obtain a valid JWT token from Supabase can authenticate with this server by including the token in the `Authorization` header of API requests:

```
Authorization: Bearer <your-jwt-token>
```

## API Endpoints

The server provides the following endpoints for integration with the music-cli application:

- `GET /health` - Health check endpoint (public)
- `GET /random` - Returns a random track ID in the Location header (authenticated)
- `GET /tracks/:id` - Stream a track (authenticated)
- `POST /prefetch` - Prefetch tracks for better performance (authenticated)
- `GET /user` - Get user information from the JWT token (authenticated)

### Request and Response Examples

#### GET /random

Request:
```
GET /random
Authorization: Bearer <your-jwt-token>
```

Response:
```
HTTP/1.1 303 See Other
Location: /tracks/track123
```

#### POST /prefetch

Request:
```
POST /prefetch
Authorization: Bearer <your-jwt-token>
Content-Type: application/json

{
  "track_ids": ["track1", "track2", "track3"]
}
```

Response:
```
HTTP/1.1 200 OK
```

## Deployment Instructions

### Portainer Deployment

When deploying to Portainer, follow these steps to avoid common issues:

1. **Environment Variables**: 
   - Set the following environment variables in Portainer:
     - `SUPABASE_JWT_SECRET`: Your JWT secret for authentication (from Supabase project settings)
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
