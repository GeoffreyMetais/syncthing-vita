#!/usr/bin/env bash
# Runs Vita3K with the given arguments, e.g. `scripts/vita3k_launch.sh -z --frontend sdl -r SYNCTHING`.
#
# No Sony firmware is needed: the app only uses HLE modules and embeds its own fonts.
# Without an X display (containers, CI, SSH), Vita3K runs in a virtual one via `xvfb-run`.
set -euo pipefail

VITA3K_BIN="${VITA3K_BIN:-$HOME/.local/bin/Vita3K}"
export SDL_VIDEODRIVER="${SDL_VIDEODRIVER:-x11}"

if [[ ! -x "$VITA3K_BIN" ]]; then
    echo "Vita3K not found at $VITA3K_BIN (run: ./scripts/setup_env.sh --install-vita3k)" >&2
    exit 1
fi

if [[ -z "${DISPLAY:-}" ]]; then
    if ! command -v xvfb-run >/dev/null 2>&1; then
        echo "No X display and xvfb-run is missing (install the xvfb package)." >&2
        exit 1
    fi
    exec xvfb-run -a -s "-screen 0 1280x720x24" "$VITA3K_BIN" "$@"
fi
exec "$VITA3K_BIN" "$@"
