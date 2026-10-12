FROM rust:1.85-slim AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY .cargo .cargo
COPY src src
COPY tests tests
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /src/target/release/page-canary /usr/local/bin/page-canary
COPY targets.json ./
COPY fixtures fixtures
COPY eval eval
COPY site site
ENV CHECK_MINUTES=60
CMD ["page-canary", "serve"]
