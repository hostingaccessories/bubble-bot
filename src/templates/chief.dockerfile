# Install Claude Code (includes Chief) via npm
RUN if ! command -v node > /dev/null 2>&1; then \
        curl -fsSL https://deb.nodesource.com/setup_22.x | bash - && \
        apt-get install -y --no-install-recommends nodejs && \
        rm -rf /var/lib/apt/lists/*; \
    fi && \
    npm install -g @anthropic-ai/claude-code
