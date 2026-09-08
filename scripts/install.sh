#!/usr/bin/env bash
# 构建并安装 human-in-loop 到 ~/.local/bin（macOS / Linux）。
# 若 cwd 在已 `dev enable` 的 worktree 内且未传 --global，则装到该树
# `.askhuman-dev/bin`（见 docs/specs/dev-instance-parallel.md）。
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

FORCE_GLOBAL=0
BUILD_PROFILE="local-install"
for arg in "$@"; do
  case "$arg" in
    --global) FORCE_GLOBAL=1 ;;
    --release) BUILD_PROFILE="release" ;;
    -h|--help)
      cat <<'EOF'
Usage: ./scripts/install.sh [--global] [--release]

  (default)  If the current directory is under a Dev Instance
             (worktree with .askhuman-dev/enabled), install into
             <root>/.askhuman-dev/bin; otherwise ~/.local/bin. Uses the
             fast local `local-install` Cargo profile.
  --global   Always install to ~/.local/bin (or $INSTALL_DIR if set).
  --release  Build with the production `release` profile instead.

Environment:
  INSTALL_DIR   Explicit install directory (wins over auto-detect;
                with --global, still defaults to ~/.local/bin when unset).
  CODESIGN_IDENTITY
                Explicit Apple Development / Developer ID identity. On macOS,
                the dedicated "human-in-loop Local Code Signing" identity is
                used when this is unset. Ad-hoc production install is refused.
EOF
      exit 0
      ;;
    *)
      echo "错误: 未知参数 $arg（可用 --global、--release 或 --help）" >&2
      exit 1
      ;;
  esac
done

find_dev_enabled_root() {
  local dir
  dir="$(pwd)"
  while [ -n "$dir" ] && [ "$dir" != "/" ]; do
    if [ -f "$dir/.askhuman-dev/enabled" ]; then
      printf '%s\n' "$dir"
      return 0
    fi
    dir="$(dirname "$dir")"
  done
  return 1
}

DEFAULT_INSTALL_DIR="${HOME}/.local/bin"
if [ -n "${INSTALL_DIR:-}" ]; then
  : # explicit env wins
elif [ "$FORCE_GLOBAL" -eq 1 ]; then
  INSTALL_DIR="$DEFAULT_INSTALL_DIR"
elif DEV_ROOT="$(find_dev_enabled_root)"; then
  INSTALL_DIR="${DEV_ROOT}/.askhuman-dev/bin"
  mkdir -p "$INSTALL_DIR" "${DEV_ROOT}/.askhuman-dev/home"
  echo "==> Dev Instance 检测到: $DEV_ROOT"
  echo "    安装目标: $INSTALL_DIR"
else
  INSTALL_DIR="$DEFAULT_INSTALL_DIR"
fi

sign_via_gui_launchd() {
  local identity="$1"
  local target="$2"
  local sign_dir status_file log_file runner_file plist_file label service gui_rc

  mkdir -p "$REPO_ROOT/tmp"
  sign_dir="$(mktemp -d "$REPO_ROOT/tmp/HUMAN_IN_LOOP_SIGN.XXXXXX")"
  status_file="$sign_dir/status"
  log_file="$sign_dir/codesign.log"
  runner_file="$sign_dir/sign.sh"
  plist_file="$sign_dir/sign.plist"
  label="io.github.cigit-zgy.human-in-loop-sign.$$"
  service="gui/$(id -u)/$label"

  {
    echo '#!/usr/bin/env bash'
    printf '/usr/bin/codesign -i %q --force --timestamp=none --sign %q %q > %q 2>&1\n' \
      "io.github.cigit-zgy.human-in-loop" "$identity" "$target" "$log_file"
    printf 'rc=$?\nprintf "%%s\\n" "$rc" > %q\nexit "$rc"\n' "$status_file"
  } > "$runner_file"
  chmod 0700 "$runner_file"

  cat > "$plist_file" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>$label</string>
  <key>ProgramArguments</key>
  <array>
    <string>$runner_file</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
</dict>
</plist>
PLIST

  if ! launchctl bootstrap "gui/$(id -u)" "$plist_file"; then
    rm -rf "$sign_dir"
    return 1
  fi

  # A one-shot agent in the user's GUI launchd domain retains keychain access without opening
  # Terminal, even when the installer itself runs under a background Codex app-server.
  for _ in $(seq 1 300); do
    [ -f "$status_file" ] && break
    sleep 0.1
  done
  if [ ! -f "$status_file" ]; then
    echo "错误: 等待 GUI 会话正式签名超时" >&2
    launchctl bootout "$service" 2>/dev/null || true
    rm -rf "$sign_dir"
    return 1
  fi

  gui_rc="$(cat "$status_file")"
  cat "$log_file"
  launchctl bootout "$service" 2>/dev/null || true
  rm -rf "$sign_dir"
  [ "$gui_rc" = "0" ]
}

if ! command -v pnpm >/dev/null 2>&1; then
  echo "错误: 需要 pnpm（npm i -g pnpm）" >&2
  exit 1
fi
if ! command -v cargo >/dev/null 2>&1; then
  echo "错误: 需要 Rust 工具链（https://rustup.rs）" >&2
  exit 1
fi

# 在途请求提示：daemon 正服务中的提问不会被安装打断——换新会在它们完结后自动发生（graceful drain），
# 期间新提问会等待。此处只提示，不强杀。
if command -v human-in-loop >/dev/null 2>&1; then
  ACTIVE="$(human-in-loop daemon status 2>/dev/null | sed -n 's/.*requests[[:space:]]*\([0-9][0-9]*\) active.*/\1/p' | head -n1 || true)"
  if [ -n "${ACTIVE:-}" ] && [ "$ACTIVE" -gt 0 ] 2>/dev/null; then
    echo "提示: daemon 当前有 $ACTIVE 个在途请求；安装后将在它们完结后自动换新（期间新提问会等待）。"
    echo "      立即换新: human-in-loop daemon restart --force（会打断在途请求）"
  fi
fi

echo "==> 安装前端依赖"
pnpm install

node scripts/build-frontend-if-needed.mjs

echo "==> 编译 $BUILD_PROFILE 二进制（前端资源在此步骤被嵌入）"
# --features custom-protocol：生产构建必须启用，否则二进制以 dev 模式连 devUrl 导致白屏。
cargo build --profile "$BUILD_PROFILE" --manifest-path src-tauri/Cargo.toml --features custom-protocol

TARGET_ROOT="${CARGO_TARGET_DIR:-src-tauri/target}"
BIN_PATH="$TARGET_ROOT/$BUILD_PROFILE/human-in-loop"
if [ ! -f "$BIN_PATH" ]; then
  echo "错误: 未找到编译产物 $BIN_PATH" >&2
  exit 1
fi

_file_sha256() {
  local path="$1"
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$path" | awk '{print $1}'
  elif command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$path" | awk '{print $1}'
  else
    return 1
  fi
}

echo "==> 安装到 $INSTALL_DIR"
mkdir -p "$INSTALL_DIR"
INSTALLED_BIN="$INSTALL_DIR/human-in-loop"
INSTALL_STATE="$INSTALL_DIR/.human-in-loop-install-state"
IDENTITY_STATE="$INSTALL_DIR/.human-in-loop-designated-requirement"
SOURCE_HASH="$(_file_sha256 "$BIN_PATH" 2>/dev/null || true)"
SKIP_COPY=0
if [ -n "$SOURCE_HASH" ] && [ -f "$INSTALLED_BIN" ] && [ -f "$INSTALL_STATE" ]; then
  STATE_SOURCE="$(sed -n 's/^source=//p' "$INSTALL_STATE" | head -n1)"
  STATE_INSTALLED="$(sed -n 's/^installed=//p' "$INSTALL_STATE" | head -n1)"
  INSTALLED_HASH="$(_file_sha256 "$INSTALLED_BIN" 2>/dev/null || true)"
  if [ "$STATE_SOURCE" = "$SOURCE_HASH" ] && [ "$STATE_INSTALLED" = "$INSTALLED_HASH" ]; then
    if [ "$(uname)" != "Darwin" ]; then
      SKIP_COPY=1
    elif [ -s "$IDENTITY_STATE" ]; then
      EXPECTED_REQUIREMENT="$(cat "$IDENTITY_STATE")"
      if codesign --verify -R="$EXPECTED_REQUIREMENT" "$INSTALLED_BIN" 2>/dev/null; then
        SKIP_COPY=1
      fi
    fi
    if [ "$SKIP_COPY" -eq 1 ]; then
      echo "    已安装二进制内容与稳定签名未变化，跳过复制与签名"
    fi
  fi
fi

if [ "$SKIP_COPY" -eq 0 ]; then
  CANDIDATE="$INSTALL_DIR/.human-in-loop.next.$$"
  REQUIREMENT_TMP=""
  trap 'rm -f "${CANDIDATE:-}" "${REQUIREMENT_TMP:-}"' EXIT
  cp "$BIN_PATH" "$CANDIDATE"
  chmod 0755 "$CANDIDATE"

  if [ "$(uname)" = "Darwin" ]; then
    # 清除 quarantine，降低拷贝后被 Gatekeeper 拦截的概率
    xattr -d com.apple.quarantine "$CANDIDATE" 2>/dev/null || true
    # TCC follows the designated requirement. Select one stable signer deterministically; never
    # activate an ad-hoc production candidate whose DR changes with every rebuild.
    IDENTITY="${CODESIGN_IDENTITY:-}"
    if [ -z "$IDENTITY" ]; then
      IDENTITY="$(security find-identity -v -p codesigning 2>/dev/null | awk '/Developer ID Application/{print $2; exit}')"
    fi
    if [ -z "$IDENTITY" ]; then
      IDENTITY="$(security find-identity -v -p codesigning 2>/dev/null | awk '/Apple Development/{print $2; exit}')"
    fi
    if [ -z "$IDENTITY" ] && security find-identity -v -p codesigning 2>/dev/null | grep -Fq '"human-in-loop Local Code Signing"'; then
      IDENTITY="human-in-loop Local Code Signing"
    fi
    if [ -z "$IDENTITY" ]; then
      echo "错误: 未找到稳定的 macOS Code Signing identity。先运行 scripts/macos-bootstrap.sh。" >&2
      exit 1
    fi
    echo "==> 稳定签名 (identifier: io.github.cigit-zgy.human-in-loop)"
    if ! codesign -i io.github.cigit-zgy.human-in-loop --force --timestamp=none --sign "$IDENTITY" "$CANDIDATE"; then
      echo "==> 后台进程无法使用签名私钥，改由当前用户 GUI launchd 域完成签名"
      sign_via_gui_launchd "$IDENTITY" "$CANDIDATE" || {
        echo "错误: 稳定签名失败，安装已中止" >&2
        exit 1
      }
    fi
    codesign --verify --strict "$CANDIDATE"
    ACTUAL_REQUIREMENT="$(codesign -d -r- "$CANDIDATE" 2>&1 | sed -n 's/^designated => //p')"
    if [ -z "$ACTUAL_REQUIREMENT" ] || printf '%s' "$ACTUAL_REQUIREMENT" | grep -q 'cdhash'; then
      echo "错误: candidate 没有稳定 designated requirement" >&2
      exit 1
    fi

    EXPECTED_REQUIREMENT=""
    if [ -s "$IDENTITY_STATE" ]; then
      EXPECTED_REQUIREMENT="$(cat "$IDENTITY_STATE")"
    elif [ -f "$INSTALLED_BIN" ]; then
      EXPECTED_REQUIREMENT="$(codesign -d -r- "$INSTALLED_BIN" 2>&1 | sed -n 's/^designated => //p' || true)"
    fi
    if [ -n "$EXPECTED_REQUIREMENT" ] && ! codesign --verify -R="$EXPECTED_REQUIREMENT" "$CANDIDATE" 2>/dev/null; then
      if printf '%s' "$EXPECTED_REQUIREMENT" | grep -q 'cdhash' \
        && [ "${HUMAN_IN_LOOP_ALLOW_IDENTITY_MIGRATION:-0}" = "1" ] \
        && [ ! -s "$IDENTITY_STATE" ]; then
        echo "==> 一次性迁移旧 ad-hoc requester 到稳定签名 identity"
      else
        echo "错误: RUNTIME_IDENTITY_MIGRATION_REQUIRED" >&2
        exit 1
      fi
    fi
    REQUIREMENT_TMP="$IDENTITY_STATE.tmp.$$"
    printf '%s\n' "$ACTUAL_REQUIREMENT" > "$REQUIREMENT_TMP"
  fi

  mv "$CANDIDATE" "$INSTALLED_BIN"
  CANDIDATE=""
  if [ "$(uname)" = "Darwin" ]; then
    mv "$REQUIREMENT_TMP" "$IDENTITY_STATE"
  fi

  if [ -n "$SOURCE_HASH" ]; then
    INSTALLED_HASH="$(_file_sha256 "$INSTALLED_BIN" 2>/dev/null || true)"
    if [ -n "$INSTALLED_HASH" ]; then
      STATE_TMP="$INSTALL_STATE.tmp.$$"
      {
        printf 'source=%s\n' "$SOURCE_HASH"
        printf 'installed=%s\n' "$INSTALLED_HASH"
      } > "$STATE_TMP"
      mv "$STATE_TMP" "$INSTALL_STATE"
    fi
  fi
fi

# --- target/ 缓存清理 ---

# cargo-sweep 回收长期未使用的依赖 hash；profile 预算（下方）不依赖它存在。
if command -v cargo-sweep >/dev/null 2>&1; then
  echo "==> 清理 7 天未使用的 target 依赖残留"
  if ! ( cd src-tauri && cargo sweep --time 7 ); then
    echo "警告: cargo-sweep 清理失败，继续执行 profile 预算检查" >&2
  fi
fi

# Cargo 自己执行 package/profile 清理并持有 target lock，避免裸 rm 与并发构建竞争。
# 先只移除本项目产物、保留三方依赖；仍超预算才清整个 profile。
_profile_size_mb() {
  local dir="$1"
  [ -d "$dir" ] || { echo 0; return; }
  du -sk "$dir" 2>/dev/null | awk '{print int(($1 + 1023) / 1024)}'
}

_enforce_profile_budget() {
  local profile="$1" dir="$2" limit_mb="$3" size_mb after_mb
  size_mb="$(_profile_size_mb "$dir")"
  [ "$size_mb" -le "$limit_mb" ] && return 0

  echo "==> $profile 缓存 ${size_mb}MB 超过预算 ${limit_mb}MB；清理本项目产物"
  if ! cargo clean --manifest-path src-tauri/Cargo.toml -p humaninloop --profile "$profile"; then
    echo "警告: 无法清理 $profile 本项目缓存" >&2
    return 0
  fi
  after_mb="$(_profile_size_mb "$dir")"
  if [ "$after_mb" -gt "$limit_mb" ]; then
    echo "==> $profile 三方依赖缓存仍有 ${after_mb}MB；执行 profile 级清理"
    if ! cargo clean --manifest-path src-tauri/Cargo.toml --profile "$profile"; then
      echo "警告: 无法完成 $profile profile 级清理" >&2
      return 0
    fi
    after_mb="$(_profile_size_mb "$dir")"
  fi
  echo "   $profile 缓存: ${size_mb}MB → ${after_mb}MB"
}

_enforce_profile_budget "local-install" "$TARGET_ROOT/local-install" 4096
_enforce_profile_budget "dev" "$TARGET_ROOT/debug" 6144
_enforce_profile_budget "full-debug" "$TARGET_ROOT/full-debug" 6144
_enforce_profile_budget "release" "$TARGET_ROOT/release" 4096

echo "==> 完成：$INSTALL_DIR/human-in-loop"
if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
  echo "提示: $INSTALL_DIR 不在 PATH 中，请将其加入 PATH。"
fi
