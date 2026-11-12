#!/usr/bin/env bash
# Runs Vita3K with the given arguments, e.g. `scripts/vita3k_launch.sh -z --frontend sdl -r SYNCTHING`.
#
# No Sony firmware is needed: the app only uses HLE modules and embeds its own fonts.
# Works on X11, Wayland, or headless via `xvfb-run`.
set -euo pipefail

VITA3K_BIN="${VITA3K_BIN:-$HOME/.local/bin/Vita3K}"

if [[ ! -x "$VITA3K_BIN" ]]; then
    echo "Vita3K not found at $VITA3K_BIN (run: ./scripts/setup_env.sh --install-vita3k)" >&2
    exit 1
fi

export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"

# Auto-detect desktop session displays if not explicitly exported in the current shell
if [[ -z "${DISPLAY:-}" && -z "${WAYLAND_DISPLAY:-}" ]]; then
    if command -v systemctl >/dev/null 2>&1; then
        env_dump="$(systemctl --user show-environment 2>/dev/null || true)"
        if [[ -z "${DISPLAY:-}" ]]; then
            d="$(printf "%s\n" "$env_dump" | awk -F= '/^DISPLAY=/ {print $2; exit}')"
            [[ -n "$d" ]] && export DISPLAY="$d"
        fi
        if [[ -z "${WAYLAND_DISPLAY:-}" ]]; then
            w="$(printf "%s\n" "$env_dump" | awk -F= '/^WAYLAND_DISPLAY=/ {print $2; exit}')"
            [[ -n "$w" ]] && export WAYLAND_DISPLAY="$w"
        fi
        if [[ -z "${XAUTHORITY:-}" ]]; then
            a="$(printf "%s\n" "$env_dump" | awk -F= '/^XAUTHORITY=/ {print $2; exit}')"
            [[ -n "$a" ]] && export XAUTHORITY="$a"
        fi
    fi
fi

# Fallback heuristics for Wayland / X11 sockets
if [[ -z "${WAYLAND_DISPLAY:-}" && -S "${XDG_RUNTIME_DIR}/wayland-0" ]]; then
    export WAYLAND_DISPLAY="wayland-0"
fi

if [[ -z "${DISPLAY:-}" && -S "/tmp/.X11-unix/X0" ]]; then
    export DISPLAY=":0"
fi

if [[ -n "${DISPLAY:-}" && -z "${XAUTHORITY:-}" ]]; then
    found_auth="$(ls -1t "${XDG_RUNTIME_DIR}"/xauth_* 2>/dev/null | head -n1 || true)"
    if [[ -n "$found_auth" && -r "$found_auth" ]]; then
        export XAUTHORITY="$found_auth"
    elif [[ -r "$HOME/.Xauthority" ]]; then
        export XAUTHORITY="$HOME/.Xauthority"
    fi
fi

# If a graphical display is available (Wayland or X11), run directly
if [[ -n "${WAYLAND_DISPLAY:-}" || -n "${DISPLAY:-}" ]]; then
    if [[ -z "${SDL_VIDEODRIVER:-}" ]]; then
        if [[ -n "${WAYLAND_DISPLAY:-}" && -z "${DISPLAY:-}" ]]; then
            export SDL_VIDEODRIVER="wayland"
        fi
    fi
    exec "$VITA3K_BIN" "$@"
fi

# Fallback for headless environments (CI, SSH without display)
if ! command -v xvfb-run >/dev/null 2>&1; then
    echo "No graphical display detected (DISPLAY or WAYLAND_DISPLAY) and xvfb-run is missing." >&2
    echo "  - If running on a desktop, verify DISPLAY (e.g. export DISPLAY=:0) or WAYLAND_DISPLAY is set." >&2
    echo "  - If running headless (CI/SSH), install xvfb (e.g. sudo apt install xvfb)." >&2
    exit 1
fi

export SDL_VIDEODRIVER="${SDL_VIDEODRIVER:-x11}"
exec xvfb-run -a -s "-screen 0 1280x720x24" "$VITA3K_BIN" "$@"
