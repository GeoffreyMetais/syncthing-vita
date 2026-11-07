set shell := ["bash", "-euo", "pipefail", "-c"]

title_id := "SYNCTHING"
target := "armv7-sony-vita-newlibeabihf"
vpk_path := "target/" + target + "/release/" + title_id + ".vpk"
vita3k_bin := env_var_or_default("VITA3K_BIN", env_var("HOME") + "/.local/bin/Vita3K")
vita3k_ux0 := env_var_or_default("VITA3K_UX0", env_var("HOME") + "/.local/share/Vita3K/Vita3K/ux0")

# Display available tasks
default:
    @just --list

# Run unit tests for BEP / Discovery logic natively on host (x86_64)
test-host:
    cargo test -p syncthing-core

# Build binary on Linux host to quickly test networking flow
run-host:
    cargo run -p vita-syncthing-app

# Cross-compile for PS Vita target and generate .vpk package
vpk:
    VITASDK="${VITASDK:-/usr/local/vitasdk}" PATH="${VITASDK:-/usr/local/vitasdk}/bin:$PATH" ./scripts/build_vpk.sh

# Download and automatically install both official Sony firmwares into Vita3K
setup-firmware:
    mkdir -p /tmp/vita-fw
    curl -fL http://dus01.psp2.update.playstation.net/update/psp2/image/2022_0209/rel_f2c7b12fe85496ec88a0391b514d6e3b/PSP2UPDAT.PUP -o /tmp/vita-fw/PSP2UPDAT.PUP
    curl -fL http://dus01.psv.update.playstation.net/update/psv/image/2012_0208/sd_2f1289b2d864758a09225d2789fa3996/PSP2UPDAT.PUP -o /tmp/vita-fw/FONT_PSP2UPDAT.PUP
    {{vita3k_bin}} --console 1 --firmware /tmp/vita-fw/PSP2UPDAT.PUP
    {{vita3k_bin}} --console 1 --firmware /tmp/vita-fw/FONT_PSP2UPDAT.PUP

# Build .vpk, deploy to ux0:app/SYNCTHING and run inside Vita3K with log streaming
run-vita3k: vpk
    @echo "Deploying {{vpk_path}} to {{vita3k_ux0}}/app/{{title_id}}..."
    mkdir -p "{{vita3k_ux0}}/app/{{title_id}}"
    unzip -qo "{{vpk_path}}" -d "{{vita3k_ux0}}/app/{{title_id}}"
    @echo "Running {{title_id}} in Vita3K (SDL mode + stdout streaming)..."
    {{vita3k_bin}} --console 1 --frontend sdl -l 2 -r {{title_id}}
