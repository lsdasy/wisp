#!/usr/bin/env bash
# Установщик Lumen для macOS (и Linux — AppImage).
#
#   Последний релиз:
#     curl -fsSL https://raw.githubusercontent.com/lsdasy/lumen/main/install.sh | bash
#
#   С параметрами:
#     curl -fsSL https://raw.githubusercontent.com/lsdasy/lumen/main/install.sh | bash -s -- --source
#     ./install.sh --version v0.1.0
#
#   Параметры:
#     --repo owner/name   репозиторий (по умолчанию lsdasy/lumen или $LUMEN_REPO)
#     --version TAG       тег релиза (по умолчанию latest)
#     --source            собрать из исходников (нужны git, cargo, node 18+, Xcode CLT)
#     --branch NAME       ветка для --source (main)
#     --dir PATH          куда клонировать для --source (~/.lumen/src)
set -euo pipefail

REPO="${LUMEN_REPO:-lsdasy/lumen}"
VERSION="latest"
FROM_SOURCE=0
BRANCH="main"
SRC_DIR="${HOME}/.lumen/src"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo)    REPO="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    --source)  FROM_SOURCE=1; shift ;;
    --branch)  BRANCH="$2"; shift 2 ;;
    --dir)     SRC_DIR="$2"; shift 2 ;;
    -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
    *) echo "Неизвестный параметр: $1" >&2; exit 1 ;;
  esac
done

c_step=$'\033[36m'; c_ok=$'\033[32m'; c_err=$'\033[31m'; c_warn=$'\033[33m'; c_off=$'\033[0m'
step() { printf '%s==>%s %s\n' "$c_step" "$c_off" "$*"; }
ok()   { printf '  %s✓%s %s\n' "$c_ok" "$c_off" "$*"; }
warn() { printf '  %s!%s %s\n' "$c_warn" "$c_off" "$*"; }
fail() { printf '  %s✗%s %s\n' "$c_err" "$c_off" "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS" in
  Darwin) PLATFORM=macos ;;
  Linux)  PLATFORM=linux ;;
  *) fail "Неподдерживаемая ОС: $OS. Для Windows используй install.ps1." ;;
esac
case "$ARCH" in
  arm64|aarch64) ARCH_TAG=aarch64; DEB_ARCH=arm64 ;;
  x86_64|amd64)  ARCH_TAG=x64;     DEB_ARCH=amd64 ;;
  *) fail "Неподдерживаемая архитектура: $ARCH" ;;
esac

sha256() { if have shasum; then shasum -a 256 "$1"; else sha256sum "$1"; fi | awk '{print $1}'; }

CURL_AUTH=()
[[ -n "${GITHUB_TOKEN:-}" ]] && CURL_AUTH=(-H "Authorization: Bearer ${GITHUB_TOKEN}")

TMP="$(mktemp -d)"
cleanup() {
  [[ -n "${MOUNT:-}" ]] && hdiutil detach "$MOUNT" -quiet >/dev/null 2>&1 || true
  rm -rf "$TMP"
}
trap cleanup EXIT

# Копирует .app в /Applications (через sudo, если нет прав)
install_app() {
  local app="$1" dest="/Applications/$(basename "$1")"
  local SUDO=""; [[ -w /Applications ]] || SUDO="sudo"
  [[ -d "$dest" ]] && $SUDO rm -rf "$dest"
  $SUDO cp -R "$app" /Applications/
  ok "Установлено в $dest"
}

# ---------------------------------------------------------------------------
# Режим 1: готовый релиз
# ---------------------------------------------------------------------------
install_release() {
  local api
  if [[ "$VERSION" == "latest" ]]; then
    api="https://api.github.com/repos/${REPO}/releases/latest"
  else
    api="https://api.github.com/repos/${REPO}/releases/tags/${VERSION}"
  fi

  step "Ищу релиз ${VERSION} в ${REPO}"
  local json
  json="$(curl -fsSL -H 'User-Agent: lumen-installer' -H 'Accept: application/vnd.github+json' ${CURL_AUTH[@]+"${CURL_AUTH[@]}"} "$api")" \
    || fail "Релиз не найден. Проверь, что есть опубликованный Release, или запусти с --source."
  local tag
  tag="$(printf '%s' "$json" | grep -m1 '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')"
  ok "Найден ${tag}"

  # Все ссылки на файлы релиза (без jq)
  local urls
  urls="$(printf '%s' "$json" | grep '"browser_download_url"' | sed -E 's/.*"(https:[^"]+)".*/\1/')"

  local url=""
  if [[ "$PLATFORM" == macos ]]; then
    # Tauri: Lumen_0.1.0_aarch64.dmg / Lumen_0.1.0_x64.dmg / Lumen_0.1.0_universal.dmg
    url="$(printf '%s\n' "$urls" | grep -E "_${ARCH_TAG}\.dmg$" | head -n1 || true)"
    [[ -z "$url" ]] && url="$(printf '%s\n' "$urls" | grep -E "_universal\.dmg$" | head -n1 || true)"
  else
    url="$(printf '%s\n' "$urls" | grep -E "_${DEB_ARCH}\.AppImage$" | head -n1 || true)"
  fi
  [[ -z "$url" ]] && fail "В релизе нет сборки для ${PLATFORM}/${ARCH_TAG}."

  local name file
  name="$(basename "$url")"
  file="${TMP}/${name}"
  step "Скачиваю ${name}"
  curl -fL --progress-bar -H 'User-Agent: lumen-installer' -o "$file" "$url"

  local sums_url
  sums_url="$(printf '%s\n' "$urls" | grep -E '/SHA256SUMS(\.txt)?$' | head -n1 || true)"
  if [[ -n "$sums_url" ]]; then
    step "Проверяю SHA-256"
    local expected actual
    expected="$(curl -fsSL -H 'User-Agent: lumen-installer' "$sums_url" | grep -F "$name" | awk '{print tolower($1)}' | head -n1)"
    [[ -z "$expected" ]] && fail "${name} не найден в SHA256SUMS."
    actual="$(sha256 "$file")"
    [[ "$expected" == "$actual" ]] || fail "Контрольная сумма не совпала — файл повреждён или подменён."
    ok "Контрольная сумма совпала"
  else
    warn "В релизе нет SHA256SUMS — проверка целостности пропущена"
  fi

  if [[ "$PLATFORM" == macos ]]; then
    step "Устанавливаю"
    MOUNT="${TMP}/mnt"
    mkdir -p "$MOUNT"
    hdiutil attach "$file" -nobrowse -readonly -mountpoint "$MOUNT" -quiet
    local app
    app="$(find "$MOUNT" -maxdepth 1 -name '*.app' | head -n1)"
    [[ -z "$app" ]] && fail "В образе не найдено .app"
    install_app "$app"
    hdiutil detach "$MOUNT" -quiet; MOUNT=""
  else
    step "Устанавливаю AppImage"
    mkdir -p "${HOME}/.local/bin"
    install -m 755 "$file" "${HOME}/.local/bin/lumen"
    ok "Установлено в ~/.local/bin/lumen"
  fi
  ok "Lumen ${tag} установлен."
}

# ---------------------------------------------------------------------------
# Режим 2: сборка из исходников
# ---------------------------------------------------------------------------
install_source() {
  step "Проверяю зависимости"
  local missing=()
  have git   || missing+=("git")
  have cargo || missing+=("Rust (cargo) → curl https://sh.rustup.rs -sSf | sh")
  have node  || missing+=("Node.js 18+ → brew install node")
  if [[ "$PLATFORM" == macos ]] && ! xcode-select -p >/dev/null 2>&1; then
    missing+=("Xcode Command Line Tools → xcode-select --install")
  fi
  if (( ${#missing[@]} )); then
    printf '  %sНе хватает:%s\n' "$c_err" "$c_off"
    printf '    %s\n' "${missing[@]}"
    exit 1
  fi
  local node_major
  node_major="$(node --version | sed -E 's/^v([0-9]+).*/\1/')"
  (( node_major >= 18 )) || fail "Нужен Node.js 18+, установлен $(node --version)."
  ok "git, cargo, node $(node --version)"

  if [[ -d "${SRC_DIR}/.git" ]]; then
    step "Обновляю исходники в ${SRC_DIR}"
    git -C "$SRC_DIR" fetch --depth 1 origin "$BRANCH"
    git -C "$SRC_DIR" checkout -B "$BRANCH" FETCH_HEAD
  else
    step "Клонирую https://github.com/${REPO} (${BRANCH})"
    mkdir -p "$(dirname "$SRC_DIR")"
    git clone --depth 1 --branch "$BRANCH" "https://github.com/${REPO}.git" "$SRC_DIR"
  fi

  cd "$SRC_DIR"
  step "Устанавливаю npm-зависимости"
  if [[ -f package-lock.json ]]; then npm ci; else npm install; fi

  step "Собираю (npm run tauri build) — первый раз это 5–15 минут"
  if [[ "$PLATFORM" == macos ]]; then npm run tauri build -- --bundles app; else npm run tauri build -- --bundles appimage; fi

  if [[ "$PLATFORM" == macos ]]; then
    local app
    app="$(find target/release/bundle/macos -maxdepth 1 -name '*.app' | head -n1)"
    [[ -z "$app" ]] && fail "Сборка прошла, но .app не найден."
    install_app "$app"
  else
    local img
    img="$(find target/release/bundle/appimage -maxdepth 1 -name '*.AppImage' | head -n1)"
    [[ -z "$img" ]] && fail "Сборка прошла, но AppImage не найден."
    mkdir -p "${HOME}/.local/bin"
    install -m 755 "$img" "${HOME}/.local/bin/lumen"
    ok "Установлено в ~/.local/bin/lumen"
  fi
  ok "Lumen собран и установлен из исходников."
}

printf '\n  Lumen installer\n  репозиторий: %s\n\n' "$REPO"
if (( FROM_SOURCE )); then install_source; else install_release; fi
