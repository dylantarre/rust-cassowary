FROM rust:1.76 as builder

WORKDIR /usr/src/app
COPY . .

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /usr/src/app/target/release/rusty-cassowary-server /app/
COPY --from=builder /usr/src/app/.env /app/

# Create music directory
RUN mkdir -p /app/music

ENV MUSIC_DIR=/app/music
EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3000/health || exit 1

CMD ["./rusty-cassowary-server"] 