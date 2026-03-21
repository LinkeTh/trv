#!/usr/bin/env bash
set -euo pipefail

REPO_URL="${TRV_REPO_URL:-https://github.com/LinkeTh/trv.git}"
INSTALL_ROOT="${TRV_INSTALL_ROOT:-$HOME/.local/share/trv}"
SRC_DIR="${INSTALL_ROOT}/src"
BIN_DIR="${TRV_BIN_DIR:-$HOME/.local/bin}"
BIN_PATH="${BIN_DIR}/trv"
SYSTEMD_DIR="${TRV_SYSTEMD_DIR:-$HOME/.config/systemd/user}"
SERVICE_NAME="trv-daemon.service"
SERVICE_PATH="${SYSTEMD_DIR}/${SERVICE_NAME}"

SKIP_PACMAN="${TRV_SKIP_PACMAN:-0}"
SKIP_SYSTEMD="${TRV_SKIP_SYSTEMD:-0}"
SKIP_PATH_UPDATE="${TRV_SKIP_PATH_UPDATE:-0}"

PATH_MARKER_BEGIN="# >>> trv installer >>>"
PATH_MARKER_END="# <<< trv installer <<<"

log() {
  printf '[trv-install] %s\n' "$*"
}

warn() {
  printf '[trv-install] WARNING: %s\n' "$*" >&2
}

die() {
  printf '[trv-install] ERROR: %s\n' "$*" >&2
  exit 1
}

require_supported_os() {
  if [[ ! -f /etc/os-release ]]; then
    die "cannot detect OS (/etc/os-release missing)"
  fi

  # shellcheck disable=SC1091
  source /etc/os-release
  local id_like="${ID_LIKE:-}"

  if [[ "${ID:-}" != "arch" && "${ID:-}" != "cachyos" && "${id_like}" != *arch* ]]; then
    die "unsupported distro: ${ID:-unknown}. This installer supports Arch/CachyOS only."
  fi
}

install_packages() {
  if [[ "$SKIP_PACMAN" == "1" ]]; then
    log "skipping pacman package installation (TRV_SKIP_PACMAN=1)"
    return
  fi

  if ! command -v sudo >/dev/null 2>&1; then
    die "sudo is required to install system packages"
  fi

  log "installing system dependencies via pacman"
  sudo pacman -Sy --needed --noconfirm \
    git \
    base-devel \
    rustup \
    android-tools \
    systemd
}

setup_rust_toolchain() {
  export PATH="$HOME/.cargo/bin:$PATH"

  if ! command -v rustup >/dev/null 2>&1; then
    die "rustup not found. Install rustup first or run without TRV_SKIP_PACMAN=1"
  fi

  log "ensuring Rust stable toolchain is installed"
  rustup toolchain install stable --profile minimal >/dev/null
  rustup default stable >/dev/null

  if ! command -v cargo >/dev/null 2>&1; then
    die "cargo not found after rustup setup"
  fi

  local rustc_version
  rustc_version="$(rustc --version | awk '{print $2}')"
  local major minor
  major="${rustc_version%%.*}"
  minor="${rustc_version#*.}"
  minor="${minor%%.*}"
  if (( major < 1 || (major == 1 && minor < 88) )); then
    die "rustc ${rustc_version} is too old. Need >= 1.88"
  fi
}

sync_source() {
  mkdir -p "$INSTALL_ROOT"

  if [[ -d "$SRC_DIR/.git" ]]; then
    log "updating existing source checkout in $SRC_DIR"
    git -C "$SRC_DIR" fetch --all --prune
    git -C "$SRC_DIR" pull --ff-only
  else
    log "cloning repository from $REPO_URL"
    git clone --depth 1 "$REPO_URL" "$SRC_DIR"
  fi
}

build_and_install_binary() {
  export PATH="$HOME/.cargo/bin:$PATH"

  log "building trv (release)"
  cargo build --release --locked --manifest-path "$SRC_DIR/Cargo.toml"

  mkdir -p "$BIN_DIR"
  install -m 755 "$SRC_DIR/target/release/trv" "$BIN_PATH"
  log "installed binary to $BIN_PATH"
}

install_user_service() {
  mkdir -p "$SYSTEMD_DIR"

  cat >"$SERVICE_PATH" <<EOF
[Unit]
Description=TRV LCD daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=${BIN_PATH} daemon --adb-forward
Restart=on-failure
RestartSec=2
WorkingDirectory=%h
Environment=PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:%h/.local/bin:%h/.cargo/bin

[Install]
WantedBy=default.target
EOF

  if [[ "$SKIP_SYSTEMD" == "1" ]]; then
    log "skipping systemd enable/start (TRV_SKIP_SYSTEMD=1)"
    return
  fi

  log "reloading user systemd daemon"
  if ! systemctl --user daemon-reload; then
    warn "systemctl --user daemon-reload failed; service file was still written"
    return
  fi

  log "enabling and starting ${SERVICE_NAME}"
  if ! systemctl --user enable --now "$SERVICE_NAME"; then
    warn "could not enable/start ${SERVICE_NAME}. Try manually after login."
  fi
}

ensure_path() {
  if [[ "$SKIP_PATH_UPDATE" == "1" ]]; then
    log "skipping shell PATH updates (TRV_SKIP_PATH_UPDATE=1)"
    return
  fi

  if [[ ":$PATH:" == *":$BIN_DIR:"* ]]; then
    return
  fi

  local snippet
  snippet="${PATH_MARKER_BEGIN}
export PATH=\"$BIN_DIR:\$PATH\"
${PATH_MARKER_END}"

  local rc
  for rc in "$HOME/.bashrc" "$HOME/.zshrc" "$HOME/.profile"; do
    [[ -f "$rc" ]] || touch "$rc"
    if ! grep -Fq "$PATH_MARKER_BEGIN" "$rc"; then
      printf '\n%s\n' "$snippet" >>"$rc"
      log "updated PATH in $rc"
    fi
  done

  export PATH="$BIN_DIR:$PATH"
}

main() {
  require_supported_os
  install_packages
  setup_rust_toolchain
  sync_source
  build_and_install_binary
  install_user_service
  ensure_path

  log "installation complete"
  log "binary: $BIN_PATH"
  log "run now: trv tui"
  if [[ "$SKIP_SYSTEMD" != "1" ]]; then
    log "service status: systemctl --user status ${SERVICE_NAME}"
  fi
}

main "$@"
