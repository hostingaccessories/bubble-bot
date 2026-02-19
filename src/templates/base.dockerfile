FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
    git \
    curl \
    wget \
    unzip \
    build-essential \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

ENV HOME=/home/dev

# Install Claude Code
# Ensure Claude Code is on PATH for all shells
ENV PATH="/home/dev/.local/bin:${PATH}"
RUN echo 'export PATH="$HOME/.local/bin:$PATH"' > /etc/profile.d/claude.sh
RUN curl -fsSL https://claude.ai/install.sh | bash
RUN mkdir -p /home/dev/.claude && chmod -R 777 /home/dev

WORKDIR /workspace
