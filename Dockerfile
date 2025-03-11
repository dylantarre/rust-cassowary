FROM rust:1.76 as builder

WORKDIR /usr/src/app
COPY . .

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    alsa-utils \
    libasound2-dev \
    libpulse0 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /usr/src/app/target/release/rusty-cassowary /app/
# Instead of copying the .env file, we'll use environment variables
# COPY --from=builder /usr/src/app/.env /app/

# Create music directory
RUN mkdir -p /app/music

ENV MUSIC_DIR=/app/music
ENV PORT=3500
ENV RUST_LOG=info
EXPOSE 3500

HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3500/health || exit 1

CMD ["./rusty-cassowary"]