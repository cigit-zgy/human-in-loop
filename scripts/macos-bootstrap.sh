#!/usr/bin/env bash
# One-time, bounded macOS setup for the stable local signer and Bot worker path.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOCAL_IDENTITY="human-in-loop Local Code Signing"
LOCAL_BINARY="${HOME}/.local/bin/human-in-loop"
SHARED_ROOT="/Users/Shared/human-in-loop"
SHARED_BINARY="/Users/Shared/human-in-loop/bin/human-in-loop"
SHARED_REQUIREMENT="/Users/Shared/human-in-loop/.human-in-loop-designated-requirement"
WORKER_LABEL="io.github.cigit-zgy.human-in-loop.imessage-worker"

usage() {
  printf 'Usage: %s --coordinator-user <short-name>\n' "$0" >&2
  exit 2
}

[ "$(uname)" = "Darwin" ] || {
  echo "错误: macOS bootstrap 只能在 macOS 上运行" >&2
  exit 1
}
[ "$#" -eq 2 ] && [ "$1" = "--coordinator-user" ] || usage
COORDINATOR_USER="$2"
case "$COORDINATOR_USER" in
  ""|*[!A-Za-z0-9_-]*)
    echo "错误: coordinator user short name 无效" >&2
    exit 1
    ;;
esac
[ "$(id -un)" = "$COORDINATOR_USER" ] || {
  echo "错误: bootstrap 必须由 coordinator user 自己启动" >&2
  exit 1
}

BOT_UID="$(/usr/bin/id -u human-in-loop)" || {
  echo "错误: human-in-loop Bot user 不存在" >&2
  exit 1
}
case "$BOT_UID" in
  ""|*[!0-9]*)
    echo "错误: Bot user id 无效" >&2
    exit 1
    ;;
esac

mkdir -p "$REPO_ROOT/tmp"
SIGNING_TMP="$(mktemp -d "$REPO_ROOT/tmp/HUMAN_IN_LOOP_BOOTSTRAP.XXXXXX")"
cleanup() {
  for file in "$SIGNING_TMP/private-key.pem" "$SIGNING_TMP/certificate.pem" \
    "$SIGNING_TMP/shared-requirement"; do
    [ ! -e "$file" ] || unlink "$file"
  done
  rmdir "$SIGNING_TMP" 2>/dev/null || true
}
trap cleanup EXIT
umask 077

AVAILABLE_IDENTITIES="$(security find-identity -v -p codesigning 2>/dev/null || true)"
if ! printf '%s\n' "$AVAILABLE_IDENTITIES" | grep -Eq \
  '"(Developer ID Application|Apple Development|human-in-loop Local Code Signing)'; then
  LOGIN_KEYCHAIN="$(security default-keychain -d user | tr -d ' "')"
  [ -n "$LOGIN_KEYCHAIN" ] || {
    echo "错误: 无法确定当前用户 login keychain" >&2
    exit 1
  }
  echo "==> 创建 machine-local human-in-loop Code Signing identity"
  openssl req -x509 -newkey rsa:3072 -sha256 -days 3650 -nodes \
    -subj "/CN=$LOCAL_IDENTITY/O=human-in-loop" \
    -addext "keyUsage=critical,digitalSignature" \
    -addext "extendedKeyUsage=codeSigning" \
    -keyout "$SIGNING_TMP/private-key.pem" \
    -out "$SIGNING_TMP/certificate.pem" >/dev/null 2>&1
  chmod 0600 "$SIGNING_TMP/private-key.pem"
  security import "$SIGNING_TMP/private-key.pem" -k "$LOGIN_KEYCHAIN" -P '' \
    -T /usr/bin/codesign >/dev/null
  security import "$SIGNING_TMP/certificate.pem" -k "$LOGIN_KEYCHAIN" >/dev/null
  security add-trusted-cert -r trustRoot -p codeSign -k "$LOGIN_KEYCHAIN" \
    "$SIGNING_TMP/certificate.pem"
fi

AVAILABLE_IDENTITIES="$(security find-identity -v -p codesigning 2>/dev/null || true)"
printf '%s\n' "$AVAILABLE_IDENTITIES" | grep -Eq \
  '"(Developer ID Application|Apple Development|human-in-loop Local Code Signing)' || {
  echo "错误: 稳定 Code Signing identity 不可用" >&2
  exit 1
}

echo "==> 构建、稳定签名并原子安装 coordinator runtime"
HUMAN_IN_LOOP_ALLOW_IDENTITY_MIGRATION=1 \
  "$SCRIPT_DIR/install.sh" --global --release

LOCAL_REQUIREMENT="$(codesign -d -r- "$LOCAL_BINARY" 2>&1 | sed -n 's/^designated => //p')"
[ -n "$LOCAL_REQUIREMENT" ] && ! printf '%s' "$LOCAL_REQUIREMENT" | grep -q cdhash || {
  echo "错误: local runtime 没有稳定 designated requirement" >&2
  exit 1
}
codesign --verify --strict "$LOCAL_BINARY"

CURRENT_SHARED_REQUIREMENT=""
if [ -f "$SHARED_BINARY" ]; then
  CURRENT_SHARED_REQUIREMENT="$(codesign -d -r- "$SHARED_BINARY" 2>&1 | sed -n 's/^designated => //p' || true)"
  if [ -n "$CURRENT_SHARED_REQUIREMENT" ] \
    && ! codesign --verify -R="$CURRENT_SHARED_REQUIREMENT" "$LOCAL_BINARY" 2>/dev/null \
    && ! printf '%s' "$CURRENT_SHARED_REQUIREMENT" | grep -q cdhash; then
    echo "错误: RUNTIME_IDENTITY_MIGRATION_REQUIRED" >&2
    exit 1
  fi
fi

printf '%s\n' "$LOCAL_REQUIREMENT" > "$SIGNING_TMP/shared-requirement"

shared_runtime_is_prepared() {
  [ -d "$SHARED_ROOT" ] && [ ! -L "$SHARED_ROOT" ] \
    && [ -d "$SHARED_ROOT/bin" ] && [ ! -L "$SHARED_ROOT/bin" ] \
    && [ -f "$SHARED_BINARY" ] && [ ! -L "$SHARED_BINARY" ] \
    && [ -f "$SHARED_REQUIREMENT" ] && [ ! -L "$SHARED_REQUIREMENT" ] \
    && [ -w "$SHARED_ROOT" ] && [ -w "$SHARED_ROOT/bin" ] \
    && [ -w "$SHARED_BINARY" ] && [ -w "$SHARED_REQUIREMENT" ] \
    && [ "$(/usr/bin/stat -f '%Su' "$SHARED_ROOT")" = "$COORDINATOR_USER" ] \
    && [ "$(/usr/bin/stat -f '%Su' "$SHARED_ROOT/bin")" = "$COORDINATOR_USER" ] \
    && [ "$(/usr/bin/stat -f '%Su' "$SHARED_BINARY")" = "$COORDINATOR_USER" ] \
    && [ "$(/usr/bin/stat -f '%Su' "$SHARED_REQUIREMENT")" = "$COORDINATOR_USER" ] \
    && [ "$(sed -n '1p' "$SHARED_REQUIREMENT")" = "$CURRENT_SHARED_REQUIREMENT" ]
}

if shared_runtime_is_prepared; then
  echo "==> ROUTINE_UPDATE_WITHOUT_SUDO: prepared shared runtime"
  /usr/bin/install -m 0755 "$LOCAL_BINARY" "$SHARED_BINARY.next"
  /bin/mv "$SHARED_BINARY.next" "$SHARED_BINARY"
  /usr/bin/install -m 0644 "$SIGNING_TMP/shared-requirement" "$SHARED_REQUIREMENT.next"
  /bin/mv "$SHARED_REQUIREMENT.next" "$SHARED_REQUIREMENT"
  /bin/launchctl kickstart -k "gui/$BOT_UID/$WORKER_LABEL"
else
  # Exactly one administrator authentication. Every later privileged command is non-interactive
  # and belongs to this fixed operation set; no root shell or caller-provided command is accepted.
  echo "==> ADMIN_AUTH_REQUIRED: bounded shared-runtime bootstrap"
  sudo -v
  sudo -n /usr/bin/install -d -o "$COORDINATOR_USER" -g wheel -m 0755 "$SHARED_ROOT"
  sudo -n /usr/bin/install -d -o "$COORDINATOR_USER" -g wheel -m 0755 "$SHARED_ROOT/bin"
  sudo -n /usr/bin/install -o "$COORDINATOR_USER" -g wheel -m 0755 \
    "$LOCAL_BINARY" "$SHARED_BINARY.next"
  sudo -n /bin/mv "$SHARED_BINARY.next" "$SHARED_BINARY"
  sudo -n /usr/bin/install -o "$COORDINATOR_USER" -g wheel -m 0644 \
    "$SIGNING_TMP/shared-requirement" "$SHARED_REQUIREMENT.next"
  sudo -n /bin/mv "$SHARED_REQUIREMENT.next" "$SHARED_REQUIREMENT"
  sudo -n /bin/launchctl kickstart -k "gui/$BOT_UID/$WORKER_LABEL"
fi

codesign --verify --strict "$SHARED_BINARY"
codesign --verify -R="$LOCAL_REQUIREMENT" "$SHARED_BINARY"
echo "==> 完成：stable runtime installed; Bot worker restarted"
