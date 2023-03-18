FROM rust
RUN git clone --depth=1 https://github.com/rust-lang/cargo-bisect-rustc && \
    cd cargo-bisect-rustc && \
    cargo install --path . && \
    cd - && \
    rm -rf cargo-bisect-rustc && \
    curl -L -o /usr/local/bin/icemelter https://github.com/langston-barrett/icemelter/releases/download/v0.2.0/icemelter && \
    chmod +x /usr/local/bin/icemelter
CMD bash
