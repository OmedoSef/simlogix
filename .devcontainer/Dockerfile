FROM rust:1-slim-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    git \
    curl \
    ca-certificates \
    libclang-dev \
    libgtk-3-dev \
    libxcb-render0-dev \
    libxcb-shape0-dev \
    libxcb-xfixes0-dev \
    libxkbcommon-dev \
    libssl-dev \
    libx11-dev \
    libxi-dev \
    libxcursor-dev \
    libxrandr-dev \
    libxext-dev \
    libgl1-mesa-dev \
    && rm -rf /var/lib/apt/lists/*

RUN rustup component add clippy rustfmt

ARG USERNAME=vscode
RUN useradd -ms /bin/bash $USERNAME

USER $USERNAME
WORKDIR /workspace
