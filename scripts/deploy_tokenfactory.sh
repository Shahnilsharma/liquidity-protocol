#!/bin/bash

# Backward-compatible wrapper.
# Legacy single-vault deployment is deprecated in v2 architecture.

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "[deprecated] scripts/deploy_tokenfactory.sh is deprecated for v2."
echo "[info] forwarding to scripts/deploy_v2_stack.sh ..."
exec "$SCRIPT_DIR/deploy_v2_stack.sh" "$@"
