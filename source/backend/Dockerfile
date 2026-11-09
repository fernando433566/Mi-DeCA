# DeCA Server · imagen x86_64 (linux/amd64): servidor Rust + aplicación web (puerto 8134)
#
# Construir (desde source/backend):
#   docker build --platform linux/amd64 -t deca-server:0.2.0 .

# ---------- Compilación ----------
FROM --platform=linux/amd64 rust:1.99.1-bookworm AS builder
WORKDIR /app

# Primero solo las dependencias (se cachean entre compilaciones)
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && cargo build --release --locked \
    && rm -rf src target/release/deps/deca_server* target/release/deca-server

COPY src ./src
RUN cargo build --release --locked

# ---------- Ejecución ----------
FROM --platform=linux/amd64 debian:bookworm-slim
LABEL org.opencontainers.image.title="DeCA Server" \
      org.opencontainers.image.description="Documento electrónico de Control Administrativo (DeCA): servidor y aplicación web" \
      org.opencontainers.image.version="0.2.0"

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tzdata curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home /data deca \
    && mkdir -p /data && chown deca /data

COPY --from=builder /app/target/release/deca-server /usr/local/bin/deca-server
# La web que se abre en el puerto 8134 va dentro de la imagen
COPY web /app/web

ENV DECA_PORT=8134 \
    DECA_DATA_DIR=/data \
    DECA_WEB_DIR=/app/web \
    TZ=Europe/Madrid \
    RUST_LOG=info,sqlx=warn

USER deca
VOLUME ["/data"]
EXPOSE 8134
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s \
    CMD curl -fs http://localhost:8134/health || exit 1
CMD ["deca-server"]
