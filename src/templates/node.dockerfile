# Node.js {{ node_version }} runtime
RUN curl -fsSL https://deb.nodesource.com/setup_{{ node_version }}.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*
