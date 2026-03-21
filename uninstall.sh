#!/usr/bin/env bash
set -euo pipefail

INSTALL_ROOT="${TRV_INSTALL_ROOT:-$HOME/.local/share/trv}"
SRC_DIR="${INSTALL_ROOT}/src"
BIN_DIR="${TRV_BIN_DIR:-$HOME/.local/bin}"
BIN_PATH="${BIN_DIR}/trv"
SYSTEMD_DIR="${TRV_SYSTEMD_DIR:-$HOME/.config/systemd/user}"
SERVICE_NAME="trv-daemon.service"
SERVICE_PATH="${SYSTEMD_DIR}/${SERVICE_NAME}"

PATH_MARKER_BEGIN="# >>> trv installer >>>"
PATH_MARKER_END="# <<< trv installer <<<"

log() {
  printf '[trv-uninstall] %s\n' "$*"
}

remove_service() {
  if command -v systemctl >/dev/null 2>&1; then
    systemctl --user disable --now "$SERVICE_NAME" >/dev/null 2>&1 || true
    systemctl --user daemon-reload >/dev/null 2>&1 || true
  fi

  if [[ -f "$SERVICE_PATH" ]]; then
    rm -f "$SERVICE_PATH"
    log "removed service file $SERVICE_PATH"
  fi
}

remove_binary() {
  if [[ -f "$BIN_PATH" ]]; then
    rm -f "$BIN_PATH"
    log "removed binary $BIN_PATH"
  fi
}

remove_source() {
  if [[ -d "$SRC_DIR" ]]; then
    rm -rf "$SRC_DIR"
    log "removed source checkout $SRC_DIR"
  fi

  if [[ -d "$INSTALL_ROOT" ]] && [[ -z "$(ls -A "$INSTALL_ROOT")" ]]; then
    rmdir "$INSTALL_ROOT" || true
  fi
}

cleanup_path_block_in_file() {
  local file="$1"
  [[ -f "$file" ]] || return 0

  local tmp
  tmp="$(mktemp)"
  awk -v begin="$PATH_MARKER_BEGIN" -v end="$PATH_MARKER_END" '
    $0 == begin { skip = 1; next }
    $0 == end { skip = 0; next }
    !skip { print }
  ' "$file" >"$tmp"
  mv "$tmp" "$file"
}

cleanup_path_blocks() {
  cleanup_path_block_in_file "$HOME/.bashrc"
  cleanup_path_block_in_file "$HOME/.zshrc"
  cleanup_path_block_in_file "$HOME/.profile"
}

main() {
  remove_service
  remove_binary
  remove_source
  cleanup_path_blocks
  log "uninstall complete"
}

main "$@"
