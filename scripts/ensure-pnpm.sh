#!/usr/bin/env bash
set -euo pipefail

if command -v pnpm >/dev/null 2>&1; then
  exit 0
fi

if command -v corepack >/dev/null 2>&1; then
  corepack enable
  if command -v pnpm >/dev/null 2>&1; then
    exit 0
  fi
fi

mkdir -p "$HOME/.local"
npm install -g --prefix "$HOME/.local" pnpm@10.25.0
