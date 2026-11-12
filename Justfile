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

# Run all workspace unit and integration tests on host (x86_64)
test-all:
    cargo test --workspace

# Check environment dependencies, VitaSDK, Rust nightly, and Vita3K setup
check-env:
    ./scripts/setup_env.sh --check-all

# Install full development environment (Rust nightly, Just, VitaSDK, Vita3K, Firmware)
setup:
    ./scripts/setup_env.sh --install-all

# Build binary on Linux host to quickly test networking flow
run-host:
    cargo run -p vita-syncthing-app

# Cross-check compilation for PS Vita target without packaging
check-vita:
    cargo +nightly check -Z build-std=std,panic_abort --package vita-syncthing-app --target {{target}}

# Cross-compile for PS Vita target and generate .vpk package
vpk:
    ./scripts/build_vpk.sh

# Run complete release VPK build, ELF symbol audit, and Vita3K installation
verify:
    ./.agents/skills/vita-vpk-verify/scripts/verify_and_deploy.sh

# Download and install official Sony firmware into Vita3K
setup-firmware:
    ./scripts/setup_env.sh --install-firmware

# Build .vpk, deploy to ux0:app/SYNCTHING and run inside Vita3K with log streaming
run-vita3k: vpk
    @echo "Deploying {{vpk_path}} to {{vita3k_ux0}}/app/{{title_id}}..."
    mkdir -p "{{vita3k_ux0}}/app/{{title_id}}"
    unzip -qo "{{vpk_path}}" -d "{{vita3k_ux0}}/app/{{title_id}}"
    @echo "Running {{title_id}} in Vita3K (SDL mode + stdout streaming)..."
    {{vita3k_bin}} -z --frontend sdl -l 2 -r {{title_id}}

# Build with scripted input, replay a UI script in Vita3K and save PNG frame dumps to target/ui-shots
ui-scenario script secs="15":
    CARGO_FEATURES=ui-script just vpk
    ./scripts/vita3k_scenario.sh {{script}} {{secs}}
