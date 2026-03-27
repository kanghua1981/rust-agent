# ─────────────────────────────────────────────────────────────────────────────
# Runtime image
#
# The binary is compiled locally with `./build.sh` (musl static target) before
# running `docker build`.  It has zero shared-library dependencies so we only
# need ca-certificates for outbound HTTPS certificate validation.
#
# Pre-requisite:
#   ./build.sh          # produces target/x86_64-unknown-linux-musl/release/agent
#   ./deploy.sh         # calls build.sh then docker build automatically
# ─────────────────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

# ca-certificates: needed for outbound HTTPS even though we use rustls,
#   because rustls-tls can optionally load system certs at runtime.
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy the pre-built static binary (produced by ./build.sh on the host,
# then staged into dist/ by deploy.sh so it isn't excluded by .dockerignore).
COPY dist/agent /usr/local/bin/agent

# ── Default config directory ──────────────────────────────────────────────────
# The agent looks for config files in ~/.config/rust_agent/ (XDG standard).
# We pre-populate the directory with empty placeholder files.
# Users can override by volume-mounting their own directory here:
#   -v /host/my-config:/root/.config/rust_agent
#
# API keys should be passed as environment variables, never baked into the image:
#   ANTHROPIC_API_KEY, OPENAI_API_KEY, LLM_BASE_URL, LLM_PROVIDER
ENV HOME=/root
RUN mkdir -p /root/.config/rust_agent

# Copy example configs shipped with the repo (if present).
# These act as documented defaults; users can override via volume mount:
#   -v ./my-config/system_prompt.md:/root/.config/rust_agent/system_prompt.md:ro
#   -v ./my-config/models.toml:/root/.config/rust_agent/models.toml:ro
#   -v ./my-config/mcp.toml:/root/.config/rust_agent/mcp.toml:ro
COPY --chown=root:root docker/config/ /root/.config/rust_agent/

# ── Workspace ─────────────────────────────────────────────────────────────────
# The default project directory.  Mount your code repository here.
#   -v /host/myproject:/workspace
VOLUME ["/workspace"]
WORKDIR /workspace

# ── Ports ─────────────────────────────────────────────────────────────────────
# WebSocket server port (--mode server).
EXPOSE 9527

# ── Entrypoint ────────────────────────────────────────────────────────────────
# Default: run in server mode, bind on all interfaces.
# Override CMD to use a different mode:
#   docker run ... agent --mode cli        # interactive terminal
#   docker run ... agent --mode stdio ...  # JSON-over-stdio
ENTRYPOINT ["/usr/local/bin/agent"]
CMD ["--mode", "server", "--host", "0.0.0.0", "--port", "9527","--isolation", "normal"]
