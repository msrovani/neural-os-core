#!/usr/bin/env bash
# Thin wrapper → smoke-cuda-honesty.sh
exec "$(dirname "$0")/smoke-cuda-honesty.sh" "$@"
