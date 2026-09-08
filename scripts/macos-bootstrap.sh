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
BOT_USER="human-in-loop"

usage() {
  cat <<EOF
Usage: $0 [--coordinator-user CURRENT_USER]

One-time macOS setup and readiness qualification (default Bot user: human-in-loop).
The coordinator defaults to the current primary user. Apple Account passwords and
2FA remain exclusively in Apple/macOS UI and are never accepted by this script.
EOF
}

[ "${1:-}" != "--help" ] && [ "${1:-}" != "-h" ] || {
  usage
  exit 0
}
[ "$(uname)" = "Darwin" ] || {
  echo "错误: macOS bootstrap 只能在 macOS 上运行" >&2
  exit 1
}
COORDINATOR_USER="$(id -un)"
if [ "$#" -gt 0 ]; then
  [ "$#" -eq 2 ] && [ "$1" = "--coordinator-user" ] || {
    usage >&2
    exit 2
  }
  COORDINATOR_USER="$2"
fi
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

BOT_UID="$(/usr/bin/id -u "$BOT_USER")" || {
  echo "错误: $BOT_USER Bot user 不存在" >&2
  exit 1
}
case "$BOT_UID" in
  ""|*[!0-9]*)
    echo "错误: Bot user id 无效" >&2
    exit 1
    ;;
esac
BOT_HOME="$(/usr/bin/dscl . -read "/Users/$BOT_USER" NFSHomeDirectory 2>/dev/null | /usr/bin/awk '{ print $2 }')"
case "$BOT_HOME" in
  /*) ;;
  *)
    echo "错误: Bot user home 无效" >&2
    exit 1
    ;;
esac
WORKER_PLIST="$BOT_HOME/Library/LaunchAgents/$WORKER_LABEL.plist"
bot_session_is_active() {
  /usr/bin/who | /usr/bin/awk -v user="$BOT_USER" \
    '$1 == user { found = 1 } END { exit(found ? 0 : 1) }'
}
authenticate_admin() {
  sudo -v
}
start_bot_worker() {
  sudo -n /bin/launchctl bootout "gui/$BOT_UID/$WORKER_LABEL" >/dev/null 2>&1 || true
  sudo -n /bin/launchctl bootstrap "gui/$BOT_UID" "$WORKER_PLIST"
}
bot_session_is_active || {
  echo "USER_CHECKPOINT: BOT_SESSION_LOGIN_REQUIRED" >&2
  echo "请登录一次 $BOT_USER macOS 用户并保持该会话登录，然后从当前主用户重新运行本命令。" >&2
  exit 3
}

IMSG_SOURCE="$(command -v imsg || true)"
[ -n "$IMSG_SOURCE" ] && [ "$($IMSG_SOURCE --version 2>/dev/null)" = "0.15.1" ] || {
  echo "错误: PATH 中需要外部 openclaw/imsg 0.15.1" >&2
  exit 1
}
IMSG_BUNDLE="$(dirname "$IMSG_SOURCE")/PhoneNumberKit_PhoneNumberKit.bundle"
for resource in PhoneNumberMetadata.json PrivacyInfo.xcprivacy; do
  [ -f "$IMSG_BUNDLE/$resource" ] || {
    echo "错误: imsg companion resource 缺失: $resource" >&2
    exit 1
  }
done

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
    && [ -f "$SHARED_ROOT/bin/imsg" ] && [ ! -L "$SHARED_ROOT/bin/imsg" ] \
    && [ -f "$SHARED_ROOT/bin/PhoneNumberKit_PhoneNumberKit.bundle/PhoneNumberMetadata.json" ] \
    && [ -f "$SHARED_ROOT/bin/PhoneNumberKit_PhoneNumberKit.bundle/PrivacyInfo.xcprivacy" ] \
    && [ -w "$SHARED_ROOT" ] && [ -w "$SHARED_ROOT/bin" ] \
    && [ -w "$SHARED_BINARY" ] && [ -w "$SHARED_REQUIREMENT" ] \
    && [ "$(/usr/bin/stat -f '%Su' "$SHARED_ROOT")" = "$COORDINATOR_USER" ] \
    && [ "$(/usr/bin/stat -f '%Su' "$SHARED_ROOT/bin")" = "$COORDINATOR_USER" ] \
    && [ "$(/usr/bin/stat -f '%Su' "$SHARED_BINARY")" = "$COORDINATOR_USER" ] \
    && [ "$(/usr/bin/stat -f '%Su' "$SHARED_REQUIREMENT")" = "$COORDINATOR_USER" ] \
    && [ "$(sed -n '1p' "$SHARED_REQUIREMENT")" = "$CURRENT_SHARED_REQUIREMENT" ] \
    && [ "$("$SHARED_ROOT/bin/imsg" --version 2>/dev/null)" = "0.15.1" ]
}

if shared_runtime_is_prepared; then
  echo "==> ROUTINE_UPDATE_WITHOUT_SUDO: prepared shared runtime"
  /usr/bin/install -m 0755 "$LOCAL_BINARY" "$SHARED_BINARY.next"
  /bin/mv "$SHARED_BINARY.next" "$SHARED_BINARY"
  /usr/bin/install -m 0644 "$SIGNING_TMP/shared-requirement" "$SHARED_REQUIREMENT.next"
  /bin/mv "$SHARED_REQUIREMENT.next" "$SHARED_REQUIREMENT"
  if ! "$LOCAL_BINARY" imessage-worker restart >/dev/null 2>&1; then
    echo "==> ADMIN_AUTH_REQUIRED: one-time worker control-protocol migration"
    authenticate_admin
    sudo -n -H -u "$BOT_USER" "$SHARED_BINARY" imessage-worker install \
      --coordinator-user "$COORDINATOR_USER" --defer-start
    start_bot_worker
  fi
else
  printf 'Bot iMessage sender handle: ' >&2
  IFS= read -r BOT_SENDER
  printf 'Personal iMessage recipient handle: ' >&2
  IFS= read -r RECIPIENT
  [ -n "$BOT_SENDER" ] && [ -n "$RECIPIENT" ] \
    && [ "$(printf '%s' "$BOT_SENDER" | tr '[:upper:]' '[:lower:]')" != "$(printf '%s' "$RECIPIENT" | tr '[:upper:]' '[:lower:]')" ] || {
    echo "错误: Bot sender 与 recipient 必须非空且属于不同 Apple/iMessage 身份" >&2
    exit 1
  }
  # Exactly one administrator authentication. Every later privileged command is non-interactive
  # and belongs to this fixed operation set; no root shell or caller-provided command is accepted.
  echo "==> ADMIN_AUTH_REQUIRED: bounded shared-runtime bootstrap"
  authenticate_admin
  sudo -n /usr/bin/install -d -o "$COORDINATOR_USER" -g wheel -m 0755 "$SHARED_ROOT"
  sudo -n /usr/bin/install -d -o "$COORDINATOR_USER" -g wheel -m 0755 "$SHARED_ROOT/bin"
  sudo -n /usr/bin/install -o "$COORDINATOR_USER" -g wheel -m 0755 \
    "$LOCAL_BINARY" "$SHARED_BINARY.next"
  sudo -n /bin/mv "$SHARED_BINARY.next" "$SHARED_BINARY"
  sudo -n /usr/bin/install -o "$COORDINATOR_USER" -g wheel -m 0644 \
    "$SIGNING_TMP/shared-requirement" "$SHARED_REQUIREMENT.next"
  sudo -n /bin/mv "$SHARED_REQUIREMENT.next" "$SHARED_REQUIREMENT"
  sudo -n /usr/bin/install -o "$COORDINATOR_USER" -g wheel -m 0755 \
    "$IMSG_SOURCE" "$SHARED_ROOT/bin/imsg.next"
  sudo -n /bin/mv "$SHARED_ROOT/bin/imsg.next" "$SHARED_ROOT/bin/imsg"
  sudo -n /usr/bin/install -d -o "$COORDINATOR_USER" -g wheel -m 0755 \
    "$SHARED_ROOT/bin/PhoneNumberKit_PhoneNumberKit.bundle"
  for resource in PhoneNumberMetadata.json PrivacyInfo.xcprivacy; do
    sudo -n /usr/bin/install -o "$COORDINATOR_USER" -g wheel -m 0644 \
      "$IMSG_BUNDLE/$resource" "$SHARED_ROOT/bin/PhoneNumberKit_PhoneNumberKit.bundle/$resource"
  done
  "$LOCAL_BINARY" channel set imessage --enable --recipient "$RECIPIENT" \
    --identity-mode distinct_peer
  BOT_SENDER_VALUE="$BOT_SENDER" RECIPIENT_VALUE="$RECIPIENT" node -e \
    'process.stdout.write(JSON.stringify({botSender: process.env.BOT_SENDER_VALUE, recipient: process.env.RECIPIENT_VALUE}))' \
    | sudo -n -H -u "$BOT_USER" "$SHARED_BINARY" imessage-worker install \
      --coordinator-user "$COORDINATOR_USER" --config-stdin --defer-start
  start_bot_worker
fi

for _ in 1 2 3 4 5 6 7 8 9 10; do
  WORKER_STATE="$("$LOCAL_BINARY" imessage-worker status 2>/dev/null || true)"
  case "$WORKER_STATE" in
    ready|bootstrap_required) break ;;
  esac
  sleep 1
done
case "$WORKER_STATE" in
  ready|bootstrap_required) ;;
  *)
    echo "USER_CHECKPOINT: $WORKER_STATE" >&2
    exit 3
    ;;
esac

codesign --verify --strict "$SHARED_BINARY"
codesign --verify -R="$LOCAL_REQUIREMENT" "$SHARED_BINARY"
echo "==> stable runtime installed; Bot worker restarted"

set +e
node "$SCRIPT_DIR/macos-setup.mjs" status --binary "$LOCAL_BINARY" \
  --bot-user "$BOT_USER" --repository "$REPO_ROOT"
SETUP_STATUS=$?
set -e
[ "$SETUP_STATUS" -eq 0 ] && exit 0
[ "$SETUP_STATUS" -eq 3 ] || exit "$SETUP_STATUS"

AUTOMATION_STATE="$($LOCAL_BINARY imessage-worker automation status 2>/dev/null || true)"
if [ "$AUTOMATION_STATE" != "automation_ready" ]; then
  echo "USER_CHECKPOINT: SETUP_NEEDS_TCC_CONSENT" >&2
  echo "请在 $BOT_USER 图形会话中一次性完成该稳定 worker 所需的 Full Disk Access 与 Automation → Messages 授权，然后重新运行本命令。" >&2
  exit 3
fi
WORKER_STATE="$($LOCAL_BINARY imessage-worker status 2>/dev/null || true)"
case "$WORKER_STATE" in
  ready|bootstrap_required) ;;
  *)
    echo "USER_CHECKPOINT: $WORKER_STATE" >&2
    exit 3
    ;;
esac

printf '准备进行一次真实 iMessage notification/reply qualification。锁定 iPhone 或保持 Messages 不在前台后，按 Return 继续：' >&2
IFS= read -r _
node "$SCRIPT_DIR/macos-setup.mjs" qualify --binary "$LOCAL_BINARY" \
  --bot-user "$BOT_USER" --repository "$REPO_ROOT"
