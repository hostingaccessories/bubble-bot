# Install Chief from GitHub releases
ARG TARGETARCH
RUN CHIEF_VERSION="0.4.0" && \
    curl -fsSL "https://github.com/MiniCodeMonkey/chief/releases/download/v${CHIEF_VERSION}/chief_${CHIEF_VERSION}_linux_${TARGETARCH}.tar.gz" \
        -o /tmp/chief.tar.gz && \
    tar -xzf /tmp/chief.tar.gz -C /usr/local/bin chief && \
    chmod +x /usr/local/bin/chief && \
    rm /tmp/chief.tar.gz
