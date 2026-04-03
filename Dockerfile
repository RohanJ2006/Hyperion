# *** Stage 1: Build the Rust backend ***
FROM ubuntu:22.04 AS backend-builder

ENV DEBIAN_FRONTEND=noninteractive

# we use rm -rf /var/lib/apt/lists/* to remove cache thus reduce image size
RUN apt-get update && apt-get install -y \
    curl \
    build-essential \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install exact Rust version
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain 1.94.0
ENV PATH="/root/.cargo/bin:$PATH"

WORKDIR /app/Backend

# Copy manifests first for better layer caching
# (dependencies only rebuild when Cargo.toml/lock changes)
COPY Backend/Cargo.toml Backend/Cargo.lock ./

# Dummy build to cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && cargo build --release \
    && rm -rf src

# Now copy real source and build
COPY Backend/src ./src
# Touch main.rs so cargo knows it changed
RUN touch src/main.rs && cargo build --release


# *** Stage 2: Build the Bun frontend ***
FROM ubuntu:22.04 AS frontend-builder

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
    curl \
    ca-certificates \
    unzip \
    && rm -rf /var/lib/apt/lists/*

# Install Bun
RUN curl -fsSL https://bun.sh/install | bash
ENV PATH="/root/.bun/bin:$PATH"

WORKDIR /app/frontend

# Copy package files first for layer caching
COPY frontend/package.json frontend/bun.lock ./

RUN bun install --frozen-lockfile

# Copy rest of frontend source
COPY frontend/ ./

RUN bun run build


# *** Stage 3: Final runtime image ***
FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app/Backend

# Copy the compiled binary
COPY --from=backend-builder /app/Backend/target/release/hyperion ./hyperion

# Copy the built frontend into the path the backend expects
# Our ServeDir points to "../frontend/dist" relative to the binary
COPY --from=frontend-builder /app/frontend/dist ../frontend/dist

# Expose port as required by the problem statement
EXPOSE 8000

CMD ["./hyperion"]
