# Install Chief from GitHub releases
RUN CHIEF_VERSION="0.4.0" && \
    ARCH=$(uname -m | sed 's/x86_64/amd64/' | sed 's/aarch64/arm64/') && \
    curl -fsSL "https://github.com/MiniCodeMonkey/chief/releases/download/v${CHIEF_VERSION}/chief_${CHIEF_VERSION}_linux_${ARCH}.tar.gz" \
        -o /tmp/chief.tar.gz && \
    tar -xzf /tmp/chief.tar.gz -C /usr/local/bin chief && \
    chmod +x /usr/local/bin/chief && \
    rm /tmp/chief.tar.gz
