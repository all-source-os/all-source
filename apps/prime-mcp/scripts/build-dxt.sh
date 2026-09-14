#!/usr/bin/env bash
# Build a Claude Desktop Extension (.dxt) bundle for allsource-prime.
#
# Output: a single multi-platform .dxt that bundles the macOS Apple Silicon
# and Linux x86_64 binaries. Claude Desktop picks the right binary at install
# time via `mcp_config.platform_overrides`. Other platforms (linux-arm64,
# macos-x86_64, windows) are excluded from `compatibility.platforms` so
# Claude Desktop refuses to install instead of failing at first launch.
#
# Inputs:
#   --version <vX.Y.Z>     Version tag to stamp into the manifest (required)
#   --darwin-binary <path> Apple Silicon `allsource-prime` binary (required)
#   --linux-binary <path>  Linux x86_64 `allsource-prime` binary (required)
#   --model-dir <path>     Directory holding the five embedding-model files
#                          (required). Staged ONCE at server/model and reached
#                          via PRIME_EMBED_MODEL_DIR, so neither binary has to
#                          bake in its own 86 MB copy and neither has to fetch
#                          from HuggingFace on first recall.
#   --out <path>           Output .dxt path (default: ./allsource-prime.dxt)
#
# DXT spec: github.com/anthropics/dxt — manifest_version 0.3.

set -euo pipefail

version=""
darwin_binary=""
linux_binary=""
model_dir=""
out="./allsource-prime.dxt"

# The files fastembed needs to build the embedder with no network.
# Keep in sync with crates/allsource-prime-models/build.rs FILES.
model_files=(
  model.onnx
  tokenizer.json
  config.json
  special_tokens_map.json
  tokenizer_config.json
)

usage() {
  cat <<EOF
Usage: $0 --version vX.Y.Z --darwin-binary PATH --linux-binary PATH \\
          --model-dir PATH [--out PATH]
EOF
  exit 1
}

while [ $# -gt 0 ]; do
  case "$1" in
    --version)        version="$2"; shift 2 ;;
    --darwin-binary)  darwin_binary="$2"; shift 2 ;;
    --linux-binary)   linux_binary="$2"; shift 2 ;;
    --model-dir)      model_dir="$2"; shift 2 ;;
    --out)            out="$2"; shift 2 ;;
    -h|--help)        usage ;;
    *)                echo "Unknown arg: $1" >&2; usage ;;
  esac
done

[ -n "$version" ] || { echo "missing --version" >&2; usage; }
[ -f "$darwin_binary" ] || { echo "darwin binary not found: $darwin_binary" >&2; exit 1; }
[ -f "$linux_binary"  ] || { echo "linux binary not found: $linux_binary" >&2; exit 1; }
[ -n "$model_dir" ] || { echo "missing --model-dir" >&2; usage; }
[ -d "$model_dir" ] || { echo "model dir not found: $model_dir" >&2; exit 1; }

# Fail here rather than shipping a bundle whose embedder dies at first recall.
for f in "${model_files[@]}"; do
  [ -s "$model_dir/$f" ] || {
    echo "model dir $model_dir is missing or has an empty $f" >&2
    exit 1
  }
done

# Strip a leading "v" from the version for the manifest (semver is bare).
manifest_version="${version#v}"

stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT

# DXT layout:
#   manifest.json
#   server/darwin/allsource-prime
#   server/linux/allsource-prime
#   server/model/{model.onnx,tokenizer.json,...}   — shared by both platforms
mkdir -p "$stage/server/darwin" "$stage/server/linux" "$stage/server/model"
install -m 0755 "$darwin_binary" "$stage/server/darwin/allsource-prime"
install -m 0755 "$linux_binary"  "$stage/server/linux/allsource-prime"
for f in "${model_files[@]}"; do
  install -m 0644 "$model_dir/$f" "$stage/server/model/$f"
done

cat > "$stage/manifest.json" <<MANIFEST
{
  "manifest_version": "0.3",
  "name": "allsource-prime",
  "display_name": "AllSource Prime",
  "version": "$manifest_version",
  "description": "Event-sourced agent memory: graph + vectors + temporal recall, accessible from Claude Desktop.",
  "long_description": "AllSource Prime is a unified memory engine that turns Claude Desktop into a long-term memory system. Every fact you teach Claude becomes a node in a knowledge graph; every conversation enriches the graph. Optionally sync to the hosted Core at api.all-source.xyz so the same memory is available across machines.",
  "author": {
    "name": "all.source",
    "url": "https://www.all-source.xyz"
  },
  "repository": {
    "type": "git",
    "url": "https://github.com/all-source-os/all-source.git"
  },
  "homepage": "https://www.all-source.xyz/prime",
  "documentation": "https://www.all-source.xyz/docs/prime",
  "support": "https://github.com/all-source-os/all-source/issues",
  "license": "Apache-2.0",
  "keywords": ["mcp", "memory", "agent", "knowledge-graph", "vectors", "event-sourcing"],
  "server": {
    "type": "binary",
    "entry_point": "server/darwin/allsource-prime",
    "mcp_config": {
      "command": "\${__dirname}/server/darwin/allsource-prime",
      "args": [
        "--data-dir", "\${user_config.data_dir}",
        "--auto-inject",
        "--sync-to", "\${user_config.sync_to}",
        "--api-key", "\${user_config.api_key}"
      ],
      "env": {
        "PRIME_EMBED_MODEL_DIR": "\${__dirname}/server/model"
      },
      "platform_overrides": {
        "linux": {
          "command": "\${__dirname}/server/linux/allsource-prime",
          "args": [
            "--data-dir", "\${user_config.data_dir}",
            "--auto-inject",
            "--sync-to", "\${user_config.sync_to}",
            "--api-key", "\${user_config.api_key}"
          ],
          "env": {
            "PRIME_EMBED_MODEL_DIR": "\${__dirname}/server/model"
          }
        }
      }
    }
  },
  "compatibility": {
    "platforms": ["darwin", "linux"]
  },
  "user_config": {
    "api_key": {
      "type": "string",
      "title": "AllSource API Key",
      "description": "Generate one at https://www.all-source.xyz/connect. The free tier is enough to get started.",
      "sensitive": true,
      "required": true
    },
    "data_dir": {
      "type": "directory",
      "title": "Local data directory",
      "description": "Where Prime stores the WAL, Parquet snapshots, and projection state. Defaults to ~/.prime/memory.",
      "default": "\${HOME}/.prime/memory",
      "required": false
    },
    "sync_to": {
      "type": "string",
      "title": "Sync to remote URL",
      "description": "Hosted Core to push prime.* events to so the dashboard memory tab can see them. Set blank to keep memory local-only.",
      "default": "https://api.all-source.xyz",
      "required": false
    }
  }
}
MANIFEST

# Validate the manifest JSON before zipping — catches typos in this script.
python3 -c 'import json,sys; json.load(open(sys.argv[1]))' "$stage/manifest.json"

# .dxt is a zip with a specific extension. Need -X to strip extra fields
# so the bundle is reproducible across systems.
#
# Absolutize $out BEFORE the cd-into-stage subshell — `zip` resolves its
# output path relative to its cwd, so a relative $out like
# "dist/allsource-prime.dxt" gets resolved against $stage (the mktemp
# dir), which doesn't have a `dist/` subdir and the zip fails with
# "No such file or directory". This was a real CI failure on the first
# v0.21.5 release run (2026-05-23) — the local functional test passed
# because mktemp's tmp dir happened to be writeable from the caller's
# cwd, masking the bug.
mkdir -p "$(dirname "$out")"
case "$out" in
  /*) out_abs="$out" ;;
  *)  out_abs="$(cd "$(dirname "$out")" && pwd)/$(basename "$out")" ;;
esac
( cd "$stage" && zip -qr -X "$out_abs" . )

if [ ! -f "$out_abs" ]; then
  echo "Failed to build .dxt at $out_abs" >&2
  exit 1
fi

size=$(du -h "$out_abs" | awk '{print $1}')
echo "✓ Built $out_abs ($size)"
