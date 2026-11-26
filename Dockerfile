# syntax=docker/dockerfile:1
# Minimal eyeship image: a static musl binary on `scratch` (plan §9).
#   docker build -t eyeship .
#   docker run --rm -v "$PWD:/work" eyeship compose

FROM rust:1-alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release --locked -p eyeship-cli \
 && cp target/release/eyeship /eyeship

FROM scratch
COPY --from=builder /eyeship /eyeship
WORKDIR /work
EXPOSE 7878
ENTRYPOINT ["/eyeship"]
CMD ["--help"]
