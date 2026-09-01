FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive
ENV PATH=/root/.cargo/bin:${PATH}

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    curl \
    device-tree-compiler \
    git \
    pkg-config \
    qemu-system-misc \
    u-boot-tools \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --no-modify-path --default-toolchain none

COPY rust-toolchain.toml .
# Install the toolchain, components, and targets declared in rust-toolchain.toml.
RUN rustc --version

RUN cargo install --locked cargo-binutils@0.4.0 axconfig-gen@0.2.1

WORKDIR /workspace

CMD ["bash"]
