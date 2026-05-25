FROM ubuntu:24.04

ARG RUST_TOOLCHAIN=nightly-2026-05-11

ENV DEBIAN_FRONTEND=noninteractive
ENV PATH=/root/.cargo/bin:${PATH}

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    curl \
    git \
    pkg-config \
    qemu-system-misc \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain ${RUST_TOOLCHAIN}

RUN rustup component add rustfmt clippy llvm-tools-preview rust-src \
    && rustup target add riscv64gc-unknown-none-elf riscv64imac-unknown-none-elf

RUN cargo install cargo-binutils axconfig-gen

WORKDIR /workspace

CMD ["bash"]
