# Go {{ go_version }} runtime
RUN ARCH=$(uname -m | sed 's/x86_64/amd64/' | sed 's/aarch64/arm64/') \
    && curl -fsSL https://go.dev/dl/go{{ go_version }}.linux-${ARCH}.tar.gz | tar -C /usr/local -xz
ENV PATH=/usr/local/go/bin:$PATH \
    GOPATH=/home/dev/go
ENV PATH=$GOPATH/bin:$PATH
