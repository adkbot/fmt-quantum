# Cópia de Dockerfile.rust na raiz para o Render encontrar o Dockerfile por padrão.
# Mantenha idêntico a Dockerfile.rust (docker-compose usa Dockerfile.rust).
FROM rust:1.90-bookworm AS builder
WORKDIR /workspace

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p fmt-api -p fmt-worker

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /workspace/target/release/fmt-api /usr/local/bin/fmt-api
COPY --from=builder /workspace/target/release/fmt-worker /usr/local/bin/fmt-worker
EXPOSE 8080
# Padrão: API + worker no mesmo container (teste/paper em instância única).
# docker-compose sobrescreve o command por serviço (fmt-api / fmt-worker separados).
CMD ["sh", "-c", "fmt-worker & exec fmt-api"]
