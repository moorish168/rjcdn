#!/usr/bin/env bash
#
# Lagran CDN installer.
#
# Interactive (pick one or more components from a menu):
#   curl -fsSL https://raw.githubusercontent.com/moorish168/rjcdn/main/install.sh | sudo bash
#
# Releases come from moorish168/rjcdn unless LAGRAN_REPO (or --repo) names another
# owner/repo. LAGRAN_GITHUB_TOKEN is optional: only needed for a private repository.
#
# Non-interactive (any combination, in any order):
#   install.sh control frontend dns edge
#   install.sh all -y
#
# Port layout. Only the edge takes 80/443, so every component can share one host:
#   edge      80, 443 (public)        admin 127.0.0.1:8082
#   control   8080                    console (Caddy) 8443
#   dns       53 udp/tcp              postgresql 127.0.0.1:5432
#
# Dependencies (PostgreSQL for control, Caddy for the console) are installed
# from the distribution / vendor repositories unless --no-deps is given.
set -Eeuo pipefail

# Default for the plain `curl | sudo bash` one-liner, which cannot pass a value.
# The console's 白牌与升级源 setting can point later upgrades elsewhere.
DEFAULT_REPO="moorish168/rjcdn"
REPO="${LAGRAN_REPO:-${GITHUB_REPOSITORY:-$DEFAULT_REPO}}"
GH_TOKEN="${LAGRAN_GITHUB_TOKEN:-${GITHUB_TOKEN:-}}"
VERSION="${LAGRAN_VERSION:-latest}"
PREFIX="${LAGRAN_PREFIX:-/opt/lagran}"
BIN_DIR="${LAGRAN_BIN_DIR:-$PREFIX/bin}"
WEB_DIR="${LAGRAN_WEB_DIR:-$PREFIX/web}"
CONF_DIR="${LAGRAN_CONF_DIR:-/etc/lagran}"
DATA_DIR="${LAGRAN_DATA_DIR:-/var/lib/lagran}"
LOG_DIR="${LAGRAN_LOG_DIR:-/var/log/lagran}"
START_SERVICE=1
INSTALL_DEPS="${LAGRAN_INSTALL_DEPS:-1}"
CONFIGURE_CADDY=1
ASSUME_YES=0
INTERACTIVE=0
COMPONENTS=()
STAGE=""
TMP=""

# Settings gathered from flags, environment or the interactive wizard.
PUBLIC_ADDR="${LAGRAN_PUBLIC_IP:-${PUBLIC_IP:-}}"
CONSOLE_PORT="${LAGRAN_CONSOLE_PORT:-8443}"
CONSOLE_TLS="${LAGRAN_CONSOLE_TLS:-internal}"
CONSOLE_DOMAIN="${LAGRAN_CONSOLE_DOMAIN:-}"
CONTROL_EXPOSE="${LAGRAN_CONTROL_EXPOSE:-1}"
ADMIN_EMAIL="${LAGRAN_ADMIN_EMAIL:-}"
ADMIN_PASSWORD="${LAGRAN_ADMIN_PASSWORD:-}"
ADMIN_GENERATED=0
ADMIN_CREATED=0
ADMIN_JWT=""
CONTROL_BOOTSTRAP=0
CONTROL_PORT=8080
EDGE_NODE_TOKEN=""
EDGE_NODE_NAME="${NODE_ID:-}"
DNS_AGENT_TOKEN="${LAGRAN_DNS_AGENT_TOKEN:-}"
CONSOLE_UPSTREAM="${LAGRAN_CONSOLE_UPSTREAM:-}"
CONTROL_FINAL_BIND=""

usage() {
  cat <<'EOF'
Usage: install.sh [component...] [options]

Run without components on a terminal to choose them from a menu.

Components (one or more):
  control    lagran-control + PostgreSQL (auto-installed), port 8080
  frontend   admin console (alias: console), static files + Caddy, port 8443
  dns        lagran-dns authoritative DNS, port 53
  edge       lagran-edge, ports 80/443
  all        all four on this host

Options:
  --repo <owner/repo>   GitHub repository (or LAGRAN_REPO); default: moorish168/rjcdn
  --version <tag>       Release tag, e.g. v1.2.3; default: latest
  --prefix <dir>        Install prefix; default: /opt/lagran
  --console-port <n>    console HTTPS port; default 8443 (LAGRAN_CONSOLE_PORT)
  --no-deps             do not install PostgreSQL/Caddy packages
  --no-caddy            frontend: copy files only, leave the web server to you
  --no-start            install files/units but do not start systemd
  -y, --yes             accept defaults, never prompt
  -h, --help

Wizard / unattended settings:
  LAGRAN_GITHUB_TOKEN=...        optional, only for a private repository; stored
                                 root-only for console / edge upgrades
  LAGRAN_BUNDLE=/path/x.tar.gz   install from a local release bundle (offline)
  LAGRAN_PUBLIC_IP=...           public IPv4 of this host (auto-detected)
  LAGRAN_ADMIN_EMAIL=...         control: first administrator e-mail
  LAGRAN_ADMIN_PASSWORD=...      control: first administrator password (generated if empty)
  LAGRAN_CONTROL_EXPOSE=1|0      control: listen on 0.0.0.0:8080 so other hosts can join (default 1)
  LAGRAN_CONSOLE_DOMAIN=...      frontend: optional domain for the console
  LAGRAN_CONSOLE_TLS=internal|off frontend: self-signed HTTPS (default) or plain HTTP
  EDGE_HTTPS_LISTEN=...          edge: default 0.0.0.0:443 (empty string disables)

Environment:
  LAGRAN_REPO=<owner/repo>       GitHub repository (default moorish168/rjcdn)
  LAGRAN_VERSION=v1.2.3          Release tag
  LAGRAN_PREFIX=/opt/lagran      Install prefix
  LAGRAN_DATABASE_URL=...        control: existing PostgreSQL URL
  LAGRAN_DB_PASSWORD=...         control: password for new lagran role
  LAGRAN_DB_NAME=lagran          control: database name
  LAGRAN_DB_USER=lagran          control: role name
  LAGRAN_JWT_SECRET=...          control: override generated JWT secret
  LAGRAN_AGENT_TOKEN=...         control: global DNS/agent token
  LAGRAN_WAF_RULEPACK_SECRET=... control+edge: sign WAF rulepacks (auto-generated)
  CHALLENGE_SECRET=...           edge: shared challenge/clearance HMAC key (auto-generated)
  LAGRAN_REDIS_URL=...           control: optional Redis URL
  LAGRAN_SEED_DEMO=true|false    control: seed demo account (default false)
  LAGRAN_CONTROL_BIND=...        control: default 0.0.0.0:8080
  LAGRAN_CONTROL_PUBLIC_URL=...  control: public HTTPS URL (alias: CONTROL_PUBLIC_URL)

  AGENT_TOKEN=...                edge/dns: required node/global token
  NODE_ID=...                    edge: node id, default hostname
  CONTROL_URL=...                edge/dns: default http://127.0.0.1:8080
  AGENT_URL=...                  edge: control callback URL
  EDGE_LISTEN=...                edge: default 0.0.0.0:80
  EDGE_ADMIN_LISTEN=...          edge: default 127.0.0.1:8082
  REDIS_URL=...                  edge: optional shared CC sliding-window Redis
  CC_REDIS_PREFIX=...            edge: default lagran:cc
  CC_REDIS_TIMEOUT_MS=...        edge: cross-node CC Redis budget (default 50)
  RATE_LIMIT_REDIS_PREFIX=...    edge: default lagran:rate
  LARGE_CACHE_ENABLED=true|false edge: large-object disk cache (default false)
  LARGE_CACHE_DIR=...            edge: default /var/lib/lagran/large-cache
  LARGE_CACHE_MAX_OBJECT_BYTES=  edge: default 1 GiB
  LARGE_CACHE_FILL_MAX_BYTES=    edge: default 0 (object max); per-object fill cap
  LARGE_CACHE_CHUNK_BYTES=       edge: fixed Range chunk size (default 1 MiB)
  LARGE_CACHE_FILL_QUEUE_BYTES=  edge: per-fill write queue bytes (default 128 MiB)
  LARGE_CACHE_FILL_QUEUE_TOTAL_BYTES= edge: process fill queue budget (default 256 MiB)
  LARGE_CACHE_MIN_FREE_BYTES=   edge: filesystem free-space floor (default 1 GiB)
  PUBLIC_IP=...                  edge: public IP for GeoDNS
  NODE_REGION=...                edge: region label
  LAGRAN_DNS_AGENT_TOKEN=...     dns: control plane's global AGENT_TOKEN (DNS on another host)
  DNS_LISTEN=...                 dns: default 0.0.0.0:53
  DNS_ZONE=...                   dns: only without a control plane; normally the CDN 主域
                                 is set in the console (智能 DNS → CDN 主域)
  DNS_NS_NAME=...                dns: NS names (with DNS_ZONE)
  DNS_NS_IP=...                  dns: glue IPs (with DNS_ZONE)
EOF
}

# Extract a release tarball, rejecting entries that would escape the target.
#
# `tar -xzf` honours absolute paths and `../` components, so a tampered archive
# can overwrite arbitrary files as root even when its checksum matches (a bundle
# built by a compromised CI runner sums correctly too). Only plain relative paths
# are accepted, and any link member is refused: a release bundle needs neither.
safe_extract() {
  local archive="$1" dest="$2" names verbose line found_link=0
  names="$(tar -tzf "${archive}" 2>/dev/null)" || { die "cannot list archive ${archive}"; return 1; }
  case "${names}" in
    /*) die "archive contains an absolute path; refusing to extract ${archive}"; return 1 ;;
  esac
  if grep -qE '(^|/)[.][.](/|$)' <<< "${names}"; then
    die "archive contains an entry that escapes its destination"
    return 1
  fi
  verbose="$(tar -tvzf "${archive}" 2>/dev/null)" || { die "cannot inspect archive ${archive}"; return 1; }
  # The first character of each verbose line is the member type: `-` file,
  # `d` directory, `l` symlink, `h` hardlink. Reject the link kinds. `die` only
  # exits the process in production, but return explicitly so the extract below
  # can never run after a rejection.
  while IFS= read -r line; do
    case "${line}" in
      l*|h*) found_link=1; break ;;
    esac
  done <<< "${verbose}"
  if [[ "${found_link}" -eq 1 ]]; then
    die "archive contains link entries; refusing to extract ${archive}"
    return 1
  fi
  tar --no-same-owner --no-same-permissions -xzf "${archive}" -C "${dest}"
}

die() { printf 'error: %s\n' "$*" >&2; exit 1; }
log() { printf '\n==> %s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }

add_component() {
  local c=$1 existing
  [[ "$c" == "console" ]] && c=frontend
  for existing in "${COMPONENTS[@]+"${COMPONENTS[@]}"}"; do
    [[ "$existing" == "$c" ]] && return 0
  done
  COMPONENTS+=("$c")
}

has_component() {
  local c
  for c in "${COMPONENTS[@]+"${COMPONENTS[@]}"}"; do
    [[ "$c" == "$1" ]] && return 0
  done
  return 1
}

# True when this run creates the control plane from scratch: only then can the
# installer register the first administrator (and a local edge node) itself.
control_bootstrapping() {
  has_component control && [[ ! -f "$CONF_DIR/control.env" ]] && [[ "$START_SERVICE" == "1" ]] \
    && [[ -z "${LAGRAN_SKIP_ADMIN:-}" ]]
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      control|edge|dns|frontend|console) add_component "$1"; shift ;;
      all) add_component control; add_component frontend; add_component dns; add_component edge; shift ;;
      --repo) REPO="${2:-}"; shift 2 ;;
      --repo=*) REPO="${1#*=}"; shift ;;
      --version) VERSION="${2:-}"; shift 2 ;;
      --version=*) VERSION="${1#*=}"; shift ;;
      --prefix) PREFIX="${2:-}"; shift 2 ;;
      --prefix=*) PREFIX="${1#*=}"; shift ;;
      --console-port) CONSOLE_PORT="${2:-}"; shift 2 ;;
      --console-port=*) CONSOLE_PORT="${1#*=}"; shift ;;
      # Kept for old command lines: the console is now always served by Caddy.
      --with-caddy) shift ;;
      --no-caddy) CONFIGURE_CADDY=0; shift ;;
      --no-deps) INSTALL_DEPS=0; shift ;;
      --no-start) START_SERVICE=0; shift ;;
      -y|--yes) ASSUME_YES=1; shift ;;
      -h|--help) usage; exit 0 ;;
      *) die "unknown argument: $1 (see --help)" ;;
    esac
  done
  [[ "$CONSOLE_PORT" =~ ^[0-9]+$ ]] && (( CONSOLE_PORT > 0 && CONSOLE_PORT < 65536 )) \
    || die "invalid console port: $CONSOLE_PORT"
  case "$CONSOLE_PORT" in
    80|443) die "the console must not use port ${CONSOLE_PORT}: 80/443 belong to the edge" ;;
  esac
  case "$CONSOLE_TLS" in internal|off) ;; *) die "LAGRAN_CONSOLE_TLS must be internal or off" ;; esac
  # Both end up in env files that the updaters `source` as root.
  [[ "$REPO" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || die "invalid repository: $REPO"
  [[ -z "$GH_TOKEN" || "$GH_TOKEN" =~ ^[A-Za-z0-9_]+$ ]] || die "LAGRAN_GITHUB_TOKEN contains unexpected characters"
  BIN_DIR="${LAGRAN_BIN_DIR:-$PREFIX/bin}"
  WEB_DIR="${LAGRAN_WEB_DIR:-$PREFIX/web}"
}

# --------------------------------------------------------------------------
# Terminal prompts. `curl | bash` feeds the script on stdin, so questions are
# read from /dev/tty instead.
# --------------------------------------------------------------------------

tty_available() {
  [[ "$ASSUME_YES" != "1" ]] && { : </dev/tty; } 2>/dev/null
}

# ask <var> <question> [default]
ask() {
  local __var=$1 question=$2 default=${3:-} answer
  if [[ "$INTERACTIVE" != "1" ]]; then
    printf -v "$__var" '%s' "$default"
    return
  fi
  if [[ -n "$default" ]]; then
    read -r -p "${question} [${default}]: " answer </dev/tty || answer=""
  else
    read -r -p "${question}: " answer </dev/tty || answer=""
  fi
  answer="${answer#"${answer%%[![:space:]]*}"}"
  answer="${answer%"${answer##*[![:space:]]}"}"
  printf -v "$__var" '%s' "${answer:-$default}"
}

# ask_secret <var> <question>; empty input is allowed (caller decides).
# Shown in clear text so installers can confirm what they typed (TTY only).
ask_secret() {
  local __var=$1 question=$2 answer=""
  if [[ "$INTERACTIVE" == "1" ]]; then
    read -r -p "${question}: " answer </dev/tty || answer=""
  fi
  printf -v "$__var" '%s' "$answer"
}

# confirm <question> <default y|n>
confirm() {
  local question=$1 default=${2:-y} answer
  if [[ "$INTERACTIVE" != "1" ]]; then
    [[ "$default" == "y" ]]
    return
  fi
  local hint="[Y/n]"
  [[ "$default" == "n" ]] && hint="[y/N]"
  read -r -p "${question} ${hint}: " answer </dev/tty || answer=""
  answer="${answer:-$default}"
  [[ "$answer" =~ ^[Yy] ]]
}

choose_components() {
  cat >/dev/tty <<'EOF'

  Lagran CDN 安装向导

  选择要安装到本机的组件（可多选）：
    1) 控制面      lagran-control + PostgreSQL        端口 8080
    2) 管理控制台  静态前端 + Caddy                    端口 8443
    3) 权威 DNS    lagran-dns                          端口 53
    4) 边缘节点    lagran-edge                         端口 80 / 443

    a) 全部安装（单机部署：1234）
    m) 管理端（123），边缘节点装在其他机器

EOF
  local answer token digit
  while :; do
    read -r -p "输入编号，可连写如 123 / 1234，也可用空格或逗号 [a]: " answer </dev/tty || die "no answer"
    answer="${answer:-a}"
    COMPONENTS=()
    local ok=1
    for token in ${answer//,/ }; do
      case "$token" in
        a|A) add_component control; add_component frontend; add_component dns; add_component edge ;;
        m|M) add_component control; add_component frontend; add_component dns ;;
        *[!0-9]*) ok=0 ;;
        *)
          # "1234" / "1" / "12 34" → each digit is a pick.
          [[ -n "$token" ]] || { ok=0; continue; }
          for ((i = 0; i < ${#token}; i++)); do
            digit="${token:i:1}"
            case "$digit" in
              1) add_component control ;;
              2) add_component frontend ;;
              3) add_component dns ;;
              4) add_component edge ;;
              *) ok=0 ;;
            esac
          done
          ;;
      esac
    done
    if [[ "$ok" == "1" && ${#COMPONENTS[@]} -gt 0 ]]; then
      break
    fi
    printf '无效输入：%s（例：1234、1 2 3、a、m）\n' "$answer" >/dev/tty
  done
}

# --------------------------------------------------------------------------
# Host facts
# --------------------------------------------------------------------------

is_private_v4() {
  [[ "$1" =~ ^(10\.|127\.|192\.168\.|169\.254\.|172\.(1[6-9]|2[0-9]|3[01])\.|100\.(6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7])\.) ]]
}

detect_public_ip() {
  local ip=""
  if command -v ip >/dev/null 2>&1; then
    ip="$(ip -4 route get 1.1.1.1 2>/dev/null | sed -n 's/.* src \([0-9.]*\).*/\1/p' | head -1 || true)"
  fi
  if [[ -z "$ip" ]] || is_private_v4 "$ip"; then
    local external
    external="$(curl -4 -fsS --max-time 4 https://api.ipify.org 2>/dev/null || true)"
    if [[ "$external" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
      ip="$external"
    fi
  fi
  printf '%s' "$ip"
}

# Login account: a username (letter first, then letters/digits/._-, 3-64
# chars) or an e-mail whose domain has a dot. Mirrors validate_email in
# apps/control/src/api/auth.rs, which accepts exactly these two shapes.
valid_email() {
  [[ "$1" =~ ^[^[:space:]@]+@[^[:space:]@]+\.[^[:space:]@]+$ ]] && return 0
  [[ ${#1} -ge 3 && ${#1} -le 64 && "$1" =~ ^[A-Za-z][A-Za-z0-9._-]*$ ]]
}

# Mirrors validate_password in apps/control/src/api/auth.rs.
valid_password() {
  [[ ${#1} -ge 8 && "$1" =~ [A-Za-z] && "$1" =~ [0-9] ]]
}

# Fill in everything the selected components need, asking only when a terminal
# is attached and the value was not supplied through the environment.
# Ask only what cannot be decided later or guessed: the administrator account
# and (for a node joining another host) where the control plane is. The DNS
# zone and NS servers are set in the console; lagran-dns pulls them from there.
# Everything else takes a sensible default and can be changed afterwards in the
# console or in /etc/lagran/*.env; environment variables override any default.
gather_settings() {
  if [[ -z "$PUBLIC_ADDR" ]]; then
    PUBLIC_ADDR="$(detect_public_ip)"
  fi
  if [[ -z "$PUBLIC_ADDR" ]]; then
    ask PUBLIC_ADDR "未能检测到本机公网 IP，请输入" ""
  fi
  EDGE_NODE_NAME="${EDGE_NODE_NAME:-$(hostname -s)}"

  if has_component control && [[ ! -f "$CONF_DIR/control.env" ]]; then
    # Only a login name: it never receives mail, and it can be changed later
    # in the console (系统设置 → 修改登录账号).
    while :; do
      ask ADMIN_EMAIL "管理员登录账号（回车使用默认，装好后可在控制台修改）" "${ADMIN_EMAIL:-admin}"
      valid_email "$ADMIN_EMAIL" && break
      [[ "$INTERACTIVE" == "1" ]] || die "LAGRAN_ADMIN_EMAIL must be a username (e.g. admin) or an e-mail address: $ADMIN_EMAIL"
      printf '账号格式不正确：用字母开头的用户名（如 admin）或邮箱\n' >/dev/tty
    done
    if [[ -z "$ADMIN_PASSWORD" ]]; then
      while :; do
        ask_secret ADMIN_PASSWORD "管理员密码（至少 8 位，含字母和数字；直接回车自动生成）"
        [[ -z "$ADMIN_PASSWORD" ]] && break
        valid_password "$ADMIN_PASSWORD" && break
        printf '密码至少 8 位，且必须同时包含字母和数字\n' >/dev/tty
      done
    fi
    if [[ -z "$ADMIN_PASSWORD" ]]; then
      ADMIN_PASSWORD="Lg$(rand_hex 8)9"
      ADMIN_GENERATED=1
    fi
    valid_password "$ADMIN_PASSWORD" || die "LAGRAN_ADMIN_PASSWORD needs 8+ characters with letters and digits"
  fi

  if has_component dns && [[ ! -f "$CONF_DIR/dns.env" ]]; then
    # No zone question: the CDN 主域 and its NS servers are set in the console
    # and lagran-dns pulls them from the control plane, so the service starts
    # right away and answers REFUSED until the console has a zone. DNS_ZONE in
    # the environment is only for running lagran-dns without a control plane.
    if [[ -n "${DNS_ZONE:-}" ]]; then
      DNS_ZONE="${DNS_ZONE%.}"
      [[ "$DNS_ZONE" =~ ^[A-Za-z0-9]([A-Za-z0-9.-]*[A-Za-z0-9])?\.[A-Za-z]{2,}$ ]] || die "invalid DNS_ZONE: $DNS_ZONE"
      DNS_NS_NAME="${DNS_NS_NAME:-ns1.${DNS_ZONE}}"
      DNS_NS_IP="${DNS_NS_IP:-${PUBLIC_ADDR:-127.0.0.1}}"
    fi
    if ! has_component control && [[ ! -f "$CONF_DIR/control.env" ]]; then
      # Without a control plane there is nothing to pull: insist on one rather
      # than install a DNS server that can never answer.
      while :; do
        ask CONTROL_URL "控制面地址（例如 http://10.0.0.10:8080）" "${CONTROL_URL:-}"
        [[ "${CONTROL_URL:-}" =~ ^https?://[^[:space:]/]+ ]] && break
        # Standalone use (DNS_ZONE from the environment) may go without one.
        [[ -n "${DNS_ZONE:-}" && -z "${CONTROL_URL:-}" ]] && break
        [[ "$INTERACTIVE" == "1" ]] \
          || die "dns on its own host needs CONTROL_URL=http://<control-host>:8080 (the control plane serves its zones)"
        printf '请输入控制面地址，例如 http://10.0.0.10:8080\n' >/dev/tty
      done
      if [[ -n "${CONTROL_URL:-}" ]]; then
        # The DNS uses the control plane's *global* token, the edge a per-node
        # one, so they are kept apart when both are installed in one run.
        if [[ -z "$DNS_AGENT_TOKEN" ]] && ! has_component edge; then
          DNS_AGENT_TOKEN="${AGENT_TOKEN:-}"
        fi
        while [[ -z "$DNS_AGENT_TOKEN" ]]; do
          [[ "$INTERACTIVE" == "1" ]] \
            || die "dns needs the control plane's global token: LAGRAN_DNS_AGENT_TOKEN=... (AGENT_TOKEN in the control host's /etc/lagran/control.env)"
          ask_secret DNS_AGENT_TOKEN "控制面全局 AGENT_TOKEN（见控制面机器 /etc/lagran/control.env）"
        done
      fi
    fi
  fi

  if has_component edge && [[ ! -f "$CONF_DIR/edge.env" ]] && [[ "$CONTROL_BOOTSTRAP" != "1" ]]; then
    ask CONTROL_URL "控制面地址（例如 http://10.0.0.10:8080）" "${CONTROL_URL:-}"
    if [[ -z "${AGENT_TOKEN:-}" ]]; then
      ask_secret AGENT_TOKEN "节点令牌（控制台 → 新增节点后获得）"
    fi
    EDGE_NODE_TOKEN="${AGENT_TOKEN:-}"
    [[ -n "$EDGE_NODE_TOKEN" ]] || die "edge install requires the node token (AGENT_TOKEN); create the node in the console first"
  fi
}

print_plan() {
  local c
  log "安装计划"
  for c in "${COMPONENTS[@]}"; do
    case "$c" in
      control)  printf '  - 控制面      lagran-control  %s:8080\n' "$([[ "$CONTROL_EXPOSE" == "1" ]] && echo 0.0.0.0 || echo 127.0.0.1)" ;;
      frontend) printf '  - 管理控制台  Caddy           :%s (%s)\n' "$CONSOLE_PORT" "$([[ "$CONSOLE_TLS" == "internal" ]] && echo HTTPS || echo HTTP)" ;;
      dns)      printf '  - 权威 DNS    lagran-dns      :53\n' ;;
      edge)     printf '  - 边缘节点    lagran-edge     :80 / :443\n' ;;
    esac
  done
  printf '  公网 IP     %s\n' "${PUBLIC_ADDR:-未知}"
  [[ -n "$ADMIN_EMAIL" && "$CONTROL_BOOTSTRAP" == "1" ]] && printf '  管理员      %s\n' "$ADMIN_EMAIL"
  if has_component frontend && [[ "$CONFIGURE_CADDY" == "1" ]]; then
    printf '  控制台      %s://%s:%s\n' "$([[ "$CONSOLE_TLS" == "internal" ]] && echo https || echo http)" \
      "${CONSOLE_DOMAIN:-${PUBLIC_ADDR:-<IP>}}" "$CONSOLE_PORT"
  fi
  if has_component dns && [[ ! -f "$CONF_DIR/dns.env" ]]; then
    if [[ -n "${DNS_ZONE:-}" ]]; then
      printf '  调度域      %s（NS %s → %s，写入 dns.env）\n' "$DNS_ZONE" "${DNS_NS_NAME:-}" "${DNS_NS_IP:-}"
    else
      printf '  CDN 主域    装好后在控制台「智能 DNS → CDN 主域」填写，DNS 自动同步\n'
    fi
  fi
  has_component edge && [[ ! -f "$CONF_DIR/edge.env" ]] && printf '  节点名称    %s\n' "$EDGE_NODE_NAME"
  printf '  其他设置使用默认值，安装后可在控制台或 /etc/lagran/*.env 修改\n'
  if [[ "$INTERACTIVE" == "1" ]]; then
    confirm "开始安装？" y || die "cancelled"
  fi
}

# --------------------------------------------------------------------------
# Dependencies
# --------------------------------------------------------------------------

PKG=""
detect_pkg() {
  if command -v apt-get >/dev/null 2>&1; then PKG=apt
  elif command -v dnf >/dev/null 2>&1; then PKG=dnf
  elif command -v yum >/dev/null 2>&1; then PKG=yum
  else PKG=""
  fi
}

APT_UPDATED=0
pkg_install() {
  case "$PKG" in
    apt)
      if [[ "$APT_UPDATED" != "1" ]]; then
        DEBIAN_FRONTEND=noninteractive apt-get update -qq
        APT_UPDATED=1
      fi
      DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \
        -o Dpkg::Options::=--force-confold "$@"
      ;;
    dnf) dnf install -y -q "$@" ;;
    yum) yum install -y -q "$@" ;;
    *) die "no supported package manager (apt/dnf/yum); install $* manually or pass --no-deps" ;;
  esac
}

postgres_ready() {
  command -v psql >/dev/null 2>&1 && psql_admin psql -tAc 'SELECT 1' >/dev/null 2>&1
}

ensure_postgres() {
  [[ -n "${LAGRAN_DATABASE_URL:-}" ]] && return 0
  if postgres_ready; then
    return 0
  fi
  [[ "$INSTALL_DEPS" == "1" ]] || die "PostgreSQL is not running; install it, set LAGRAN_DATABASE_URL, or drop --no-deps"
  log "Installing PostgreSQL"
  case "$PKG" in
    apt) pkg_install postgresql ;;
    dnf|yum)
      pkg_install postgresql-server postgresql
      if [[ ! -s /var/lib/pgsql/data/PG_VERSION ]] && command -v postgresql-setup >/dev/null 2>&1; then
        postgresql-setup --initdb >/dev/null
      fi
      ;;
    *) pkg_install postgresql ;;
  esac
  systemctl enable --now postgresql >/dev/null 2>&1 || true
  local _
  for _ in $(seq 1 30); do
    postgres_ready && break
    sleep 1
  done
  postgres_ready || die "PostgreSQL did not start; see: journalctl -u postgresql -n 50"
  # RHEL-family clusters default to `ident` for TCP loopback, which rejects the
  # password login the control plane uses. Switch those two lines to md5 (which
  # also accepts SCRAM-stored passwords) and reload.
  local hba
  hba="$(psql_admin psql -tAc 'SHOW hba_file' 2>/dev/null | tr -d '[:space:]' || true)"
  if [[ -n "$hba" && -f "$hba" ]] && grep -Eq '^host[[:space:]]+all[[:space:]]+all[[:space:]]+(127\.0\.0\.1/32|::1/128)[[:space:]]+ident' "$hba"; then
    sed -i -E 's/^(host[[:space:]]+all[[:space:]]+all[[:space:]]+(127\.0\.0\.1\/32|::1\/128)[[:space:]]+)ident/\1md5/' "$hba"
    psql_admin psql -tAc 'SELECT pg_reload_conf()' >/dev/null
  fi
}

ensure_caddy() {
  command -v caddy >/dev/null 2>&1 && return 0
  [[ "$INSTALL_DEPS" == "1" ]] || die "caddy is not installed; install it or drop --no-deps"
  log "Installing Caddy"
  case "$PKG" in
    apt)
      pkg_install ca-certificates curl gnupg
      if curl -1sLf https://dl.cloudsmith.io/public/caddy/stable/gpg.key \
          | gpg --dearmor --yes -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg \
        && curl -1sLf https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt \
          -o /etc/apt/sources.list.d/caddy-stable.list; then
        chmod o+r /usr/share/keyrings/caddy-stable-archive-keyring.gpg /etc/apt/sources.list.d/caddy-stable.list
        APT_UPDATED=0
      else
        warn "could not add the Caddy apt repository; trying the distribution package"
        rm -f /etc/apt/sources.list.d/caddy-stable.list
      fi
      pkg_install caddy
      ;;
    dnf)
      dnf install -y -q 'dnf-command(copr)' >/dev/null 2>&1 || true
      dnf copr enable -y @caddy/caddy >/dev/null 2>&1 || warn "could not enable the Caddy COPR; trying the default repositories"
      pkg_install caddy
      ;;
    yum)
      yum install -y -q yum-plugin-copr >/dev/null 2>&1 || true
      yum copr enable -y @caddy/caddy >/dev/null 2>&1 || warn "could not enable the Caddy COPR; trying the default repositories"
      pkg_install caddy
      ;;
    *) pkg_install caddy ;;
  esac
  command -v caddy >/dev/null 2>&1 || die "caddy installation failed"
}

# --------------------------------------------------------------------------
# Ports, firewall, systemd-resolved
# --------------------------------------------------------------------------

# Prints the process names listening on <port> for <proto> (tcp|udp).
port_owners() {
  local port=$1 proto=$2 flag=-lnt
  [[ "$proto" == "udp" ]] && flag=-lnu
  command -v ss >/dev/null 2>&1 || return 0
  ss -Hp "$flag" "sport = :${port}" 2>/dev/null \
    | { grep -o 'users:(("[^"]*"' || true; } \
    | sed -e 's/^users:(("//' -e 's/"$//' \
    | sort -u | tr '\n' ' '
}

# require_port <port> <proto> <allowed-owner-regex> <purpose>
require_port() {
  local port=$1 proto=$2 allowed=$3 purpose=$4 owners name
  owners="$(port_owners "$port" "$proto")"
  [[ -z "$owners" ]] && return 0
  for name in $owners; do
    if [[ ! "$name" =~ ^(${allowed})$ ]]; then
      die "port ${port}/${proto} (${purpose}) is already used by: ${owners}"
    fi
  done
}

FIREWALL_PORTS=()
open_port() { FIREWALL_PORTS+=("$1"); }

apply_firewall() {
  [[ ${#FIREWALL_PORTS[@]} -gt 0 ]] || return 0
  local p
  if command -v ufw >/dev/null 2>&1 && ufw status 2>/dev/null | grep -q '^Status: active'; then
    log "Opening ports in ufw: ${FIREWALL_PORTS[*]}"
    for p in "${FIREWALL_PORTS[@]}"; do ufw allow "$p" >/dev/null; done
  elif command -v firewall-cmd >/dev/null 2>&1 && firewall-cmd --state >/dev/null 2>&1; then
    log "Opening ports in firewalld: ${FIREWALL_PORTS[*]}"
    for p in "${FIREWALL_PORTS[@]}"; do firewall-cmd --permanent --add-port="$p" >/dev/null; done
    firewall-cmd --reload >/dev/null
  fi
}

# systemd-resolved's stub listener holds 127.0.0.53:53, which blocks a wildcard
# bind on :53. Turn the stub off and point resolv.conf at the real upstreams so
# this host keeps resolving names.
free_dns_port() {
  local owners
  owners="$(port_owners 53 udp) $(port_owners 53 tcp)"
  [[ "$owners" =~ systemd-resolve ]] || return 0
  log "Port 53 is held by systemd-resolved: disabling its local stub listener (this host keeps resolving through the upstream DNS)"
  install -d -m 0755 /etc/systemd/resolved.conf.d
  printf '[Resolve]\nDNSStubListener=no\n' > /etc/systemd/resolved.conf.d/lagran-no-stub.conf
  if [[ -L /etc/resolv.conf ]] && [[ "$(readlink /etc/resolv.conf)" == *stub-resolv.conf ]] \
    && [[ -f /run/systemd/resolve/resolv.conf ]]; then
    ln -sf /run/systemd/resolve/resolv.conf /etc/resolv.conf
  fi
  systemctl restart systemd-resolved
}

# Community mirror (same URL as lagran-control「更新 IP 库」).
GEOIP_COUNTRY_MIRROR_GZ_URL="${GEOIP_COUNTRY_MIRROR_GZ_URL:-https://raw.githubusercontent.com/wp-statistics/GeoLite2-Country/master/GeoLite2-Country.mmdb.gz}"

# Install MaxMind GeoLite2-Country under DATA_DIR and point control.env at it.
# Prefer a copy shipped in the release bundle; otherwise download the mirror.
ensure_geoip_country_db() {
  local dest_dir="${DATA_DIR}/maxmind"
  local dest="${dest_dir}/GeoLite2-Country.mmdb"
  install -d -m 0775 -o root -g lagran "$dest_dir" 2>/dev/null \
    || install -d -m 0775 "$dest_dir"
  # Release tarball may ship data/maxmind/GeoLite2-Country.mmdb
  local bundled=""
  if [[ -n "${STAGE:-}" ]]; then
    bundled="$(find "$STAGE" -maxdepth 4 -type f -name 'GeoLite2-Country.mmdb' -print -quit 2>/dev/null || true)"
  fi
  if [[ -n "$bundled" && -s "$bundled" ]]; then
    install -m 0664 "$bundled" "$dest"
    log "Installed GeoLite2-Country from release bundle → ${dest}"
  elif [[ -s "$dest" ]]; then
    log "Keeping existing GeoLite2-Country at ${dest}"
  else
    log "Downloading GeoLite2-Country → ${dest}"
    local tmp="${dest}.tmp.$$"
    if curl -fsSL --retry 3 --connect-timeout 20 "$GEOIP_COUNTRY_MIRROR_GZ_URL" \
      | gzip -dc > "$tmp" \
      && [[ -s "$tmp" ]] \
      && [[ "$(wc -c < "$tmp")" -gt 1024 ]]; then
      mv -f "$tmp" "$dest"
      chmod 0664 "$dest"
    else
      rm -f "$tmp"
      warn "GeoLite2-Country download failed; Smart DNS lines stay on default until「更新 IP 库」or a manual install"
      return 0
    fi
  fi
  chown root:lagran "$dest" 2>/dev/null || true
  chmod 0664 "$dest" 2>/dev/null || true
  # Group-writable so lagran-control can refresh via the console button.
  chgrp lagran "$dest_dir" 2>/dev/null || true
  chmod g+rwxs "$dest_dir" 2>/dev/null || true
}

set_env_value() {
  local file=$1 key=$2 value=$3
  if grep -q "^${key}=" "$file"; then
    sed -i "s|^${key}=.*|${key}=${value}|" "$file"
  else
    printf '%s=%s\n' "$key" "$value" >> "$file"
  fi
}

env_value() {
  sed -n "s/^$2=//p" "$1" 2>/dev/null | tail -1
}

json_escape() {
  local s=${1//\\/\\\\}
  s=${s//\"/\\\"}
  printf '%s' "$s"
}

json_field() {
  # Extract a top-level string field from a small JSON document.
  sed -n "s/.*\"$1\"[[:space:]]*:[[:space:]]*\"\([^\"]*\)\".*/\1/p" | head -1
}

wait_health() {
  local url=$1 _
  for _ in $(seq 1 30); do
    curl -fsS "$url" >/dev/null 2>&1 && return 0
    sleep 1
  done
  return 1
}

rand_hex() {
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -hex "$1"
  else
    od -An -N "$1" -tx1 /dev/urandom | tr -d ' \n'
  fi
}

psql_admin() {
  if command -v runuser >/dev/null 2>&1; then
    runuser -u postgres -- "$@"
  elif command -v sudo >/dev/null 2>&1; then
    sudo -u postgres "$@"
  else
    return 1
  fi
}

setup_database() {
  if [[ -n "${LAGRAN_DATABASE_URL:-}" ]]; then
    printf '%s' "$LAGRAN_DATABASE_URL"
    return
  fi
  command -v psql >/dev/null 2>&1 || die "psql not found; install PostgreSQL or set LAGRAN_DATABASE_URL"
  command -v runuser >/dev/null 2>&1 || command -v sudo >/dev/null 2>&1 || \
    die "cannot access postgres user; set LAGRAN_DATABASE_URL"

  local db_user="${LAGRAN_DB_USER:-lagran}"
  local db_name="${LAGRAN_DB_NAME:-lagran}"
  [[ "$db_user" =~ ^[a-zA-Z0-9_]+$ ]] || die "invalid LAGRAN_DB_USER"
  [[ "$db_name" =~ ^[a-zA-Z0-9_]+$ ]] || die "invalid LAGRAN_DB_NAME"
  local db_pass="${LAGRAN_DB_PASSWORD:-$(rand_hex 24)}"

  printf '\n==> Configuring PostgreSQL role/database\n' >&2
  psql_admin psql -v ON_ERROR_STOP=1 -q <<SQL
DO \$\$
BEGIN
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = '${db_user}') THEN
    CREATE ROLE ${db_user} LOGIN PASSWORD '${db_pass}';
  ELSE
    ALTER ROLE ${db_user} WITH LOGIN PASSWORD '${db_pass}';
  END IF;
END
\$\$;
SQL
  if ! psql_admin psql -tAc "SELECT 1 FROM pg_database WHERE datname='${db_name}'" | grep -q 1; then
    psql_admin createdb -O "$db_user" "$db_name"
  fi
  printf 'postgres://%s:%s@127.0.0.1:5432/%s' "$db_user" "$db_pass" "$db_name"
}

ensure_dirs() {
  install -d -m 0755 "$PREFIX" "$BIN_DIR" "$CONF_DIR" "$DATA_DIR" "$LOG_DIR"
}

ensure_users() {
  getent group lagran >/dev/null 2>&1 || groupadd --system lagran
  # One account per service. A single `lagran` uid meant a DNS compromise could
  # read control.env (JWT secret, agent token, database password). The shared
  # `lagran` group still allows all services to use the same data dir, while each
  # env file is owned by its own group and is therefore unreadable by the other
  # service accounts. No process is a member of those groups: systemd reads
  # EnvironmentFile as root before dropping privileges.
  local user
  for user in lagran-control lagran-edge lagran-dns; do
    getent group "$user" >/dev/null 2>&1 || groupadd --system "$user"
    if ! id -u "$user" >/dev/null 2>&1; then
      useradd --system --gid lagran --home-dir "$DATA_DIR" --shell /usr/sbin/nologin "$user"
    fi
  done
  # Keep the legacy account for existing installs; it no longer runs a service.
  id -u lagran >/dev/null 2>&1 || \
    useradd --system --gid lagran --home-dir "$DATA_DIR" --shell /usr/sbin/nologin lagran
  chown -R root:lagran "$DATA_DIR" "$LOG_DIR" 2>/dev/null || true
  chmod -R g+rwX "$DATA_DIR" "$LOG_DIR" 2>/dev/null || true
  # The control plane queues updates by creating a file in DATA_DIR. Older
  # releases left that directory root:lagran 0755 (or lagran:lagran), where the
  # split lagran-control account can only read/traverse. Make the top level
  # group-writable and setgid so newly queued files keep the shared group.
  chgrp lagran "$DATA_DIR" 2>/dev/null || true
  chmod g+rwxs "$DATA_DIR" 2>/dev/null || true
  # A previous control run as the legacy `lagran` user may have left the
  # update temp file owned by a uid the new service account cannot write.
  rm -f "$DATA_DIR"/update.request.tmp "$DATA_DIR"/update.request.tmp.* 2>/dev/null || true
  # Upgrade migration: files created by the old single `lagran` account may be
  # group-writable but owned by another user. The edge now replaces cert/key
  # files atomically through the directory, but handing it the cert/cache tree
  # also keeps `ls -l` and manual cleanup predictable.
  local sub
  for sub in certs cache large-cache; do
    if [[ -d "$DATA_DIR/$sub" ]]; then
      chown -R lagran-edge:lagran "$DATA_DIR/$sub" 2>/dev/null || true
    fi
  done
}

install_env_example() {
  local name=$1
  if [[ -f "$STAGE/$name" ]]; then
    install -m 0644 "$STAGE/$name" "$CONF_DIR/$name"
  fi
}

install_binary() {
  local name=$1
  local src dest tmp
  src="$(find "$STAGE" -maxdepth 3 -type f -name "$name" -print -quit 2>/dev/null || true)"
  if [[ -z "$src" ]]; then
    local entries
    entries="$(find "$STAGE" -maxdepth 2 -type f -printf '%P ' 2>/dev/null | cut -c1-400)"
    die "release does not contain $name${entries:+ (found: $entries)}"
  fi
  # Stage beside the destination and rename atomically. This permits an
  # in-place installer rerun while the old service is still executing the
  # previous inode; enable_service restarts it after configuration is ready.
  # GitHub artifact downloads may lose the executable bit; force 0755 here.
  dest="$BIN_DIR/$name"
  tmp="$BIN_DIR/.${name}.new.$$"
  if ! install -m 0755 "$src" "$tmp"; then
    rm -f "$tmp"
    die "could not stage $name"
  fi
  if ! mv -f "$tmp" "$dest"; then
    rm -f "$tmp"
    die "could not replace $name"
  fi
}

# --------------------------------------------------------------------------
# GitHub releases. A private repository needs a token that can read its
# contents (fine-grained token: this repository, Contents: Read-only).
# --------------------------------------------------------------------------

gh_api() {
  local auth=()
  [[ -n "$GH_TOKEN" ]] && auth=(-H "Authorization: Bearer ${GH_TOKEN}")
  curl -fsSL --retry 3 --connect-timeout 15 ${auth[@]+"${auth[@]}"} \
    -H "Accept: application/vnd.github+json" "https://api.github.com/repos/${REPO}/$1"
}

gh_latest_tag() {
  gh_api releases/latest | tr ',' '\n' \
    | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -1
}

# gh_download <tag> <asset-name> <output-file>
gh_download() {
  local tag=$1 name=$2 out=$3 url=""
  if [[ -z "$GH_TOKEN" ]]; then
    curl -fL --retry 3 --retry-delay 2 --connect-timeout 15 \
      "https://github.com/${REPO}/releases/download/${tag}/${name}" -o "$out"
    return
  fi
  # Private assets are only served through the API asset URL. curl drops the
  # Authorization header when it follows the redirect to the storage host.
  url="$(gh_api "releases/tags/${tag}" | tr ',{}' '\n\n\n' | awk -v want="$name" '
    /"url"[[:space:]]*:[[:space:]]*"https:\/\/api\.github\.com\/repos\/[^"]*\/releases\/assets\/[0-9]+"/ {
      u = $0; sub(/^[^"]*"url"[[:space:]]*:[[:space:]]*"/, "", u); sub(/".*/, "", u)
    }
    /"name"[[:space:]]*:/ {
      n = $0; sub(/^[^"]*"name"[[:space:]]*:[[:space:]]*"/, "", n); sub(/".*/, "", n)
      if (u != "" && n == want) { print u; exit }
    }' || true)"
  [[ -n "$url" ]] || return 1
  curl -fL --retry 3 --retry-delay 2 --connect-timeout 15 \
    -H "Authorization: Bearer ${GH_TOKEN}" -H "Accept: application/octet-stream" "$url" -o "$out"
}

resolve_release() {
  # Offline / air-gapped install: a bundle the operator copied over. It is
  # trusted as-is, exactly like running a binary they placed there themselves.
  if [[ -n "${LAGRAN_BUNDLE:-}" ]]; then
    [[ -f "$LAGRAN_BUNDLE" ]] || die "LAGRAN_BUNDLE does not exist: $LAGRAN_BUNDLE"
    log "Using local bundle ${LAGRAN_BUNDLE}"
    TMP="$(mktemp -d)"
    STAGE="$TMP/stage"
    mkdir -p "$STAGE"
    safe_extract "$LAGRAN_BUNDLE" "$STAGE"
    return
  fi
  log "Resolving release"
  if [[ "$VERSION" == "latest" ]]; then
    VERSION="$(gh_latest_tag || true)"
    if [[ -z "$VERSION" ]]; then
      die "could not resolve the latest release of $REPO: publish one by pushing a v* tag (or pass --version)"
    fi
  fi
  case "$(uname -m)" in
    x86_64|amd64) ARCH=amd64 ;;
    aarch64|arm64) die "prebuilt arm64 bundle is not published; build locally" ;;
    *) die "unsupported architecture: $(uname -m)" ;;
  esac
  local asset="lagran-cdn-${VERSION#v}-linux-${ARCH}.tar.gz"
  log "Downloading ${asset}"
  TMP="$(mktemp -d)"
  gh_download "$VERSION" "$asset" "$TMP/$asset" \
    || die "download of ${asset} (${REPO} ${VERSION}) failed"
  # Refuse to install an unverified release. The previous fallback turned a
  # missing or tampered SHA256SUMS into a silent downgrade to "install whatever
  # the network gave us", executed as root.
  if ! gh_download "$VERSION" SHA256SUMS "$TMP/SHA256SUMS"; then
    die "SHA256SUMS unavailable for ${VERSION}; refusing to install an unverified release"
  fi
  if ! (cd "$TMP" && grep -F "  ${asset}" SHA256SUMS | sha256sum -c - >/dev/null 2>&1); then
    die "checksum verification failed for ${asset}"
  fi
  STAGE="$TMP/stage"
  mkdir -p "$STAGE"
  safe_extract "$TMP/$asset" "$STAGE"
  # Some old bundles wrap everything in a top-level directory.
  if [[ ! -d "$STAGE/bin" && ! -f "$STAGE/lagran-control" ]]; then
    local nested
    nested="$(find "$STAGE" -mindepth 1 -maxdepth 1 -type d -print -quit 2>/dev/null || true)"
    if [[ -n "$nested" && ( -d "$nested/bin" || -f "$nested/lagran-control" ) ]]; then
      STAGE="$nested"
    fi
  fi
}

write_control_env() {
  local env_file="$CONF_DIR/control.env"
  local geoip_db="${DATA_DIR}/maxmind/GeoLite2-Country.mmdb"
  if [[ -f "$env_file" ]]; then
    log "Keeping existing $env_file"
    # Upgrades must come from this (private) repository, also on old installs.
    set_env_value "$env_file" LAGRAN_UPDATE_REPO "$REPO"
    [[ -n "$GH_TOKEN" ]] && set_env_value "$env_file" LAGRAN_GITHUB_TOKEN "$GH_TOKEN"
    # Edges installed remotely by the control plane get EDGE_CHALLENGE_SECRET;
    # adopt the local edge's secret so every edge signs clearances alike.
    # (SSH_GRANT_KEY is deliberately not added here: credentials saved before
    # it existed are sealed with the JWT-derived key and must stay readable.)
    if [[ -z "$(env_value "$env_file" EDGE_CHALLENGE_SECRET)" ]]; then
      local edge_secret=""
      [[ -f "$CONF_DIR/edge.env" ]] && edge_secret="$(env_value "$CONF_DIR/edge.env" CHALLENGE_SECRET)"
      set_env_value "$env_file" EDGE_CHALLENGE_SECRET "${edge_secret:-$(rand_hex 32)}"
    fi
    # Migrate relative / empty GeoIP paths so Smart DNS stops sitting on default.
    local cur_geo
    cur_geo="$(env_value "$env_file" GEOIP_COUNTRY_DB)"
    if [[ -z "$cur_geo" || "$cur_geo" == ./* || "$cur_geo" != /* ]]; then
      set_env_value "$env_file" GEOIP_COUNTRY_DB "$geoip_db"
    fi
    chown root:lagran-control "$env_file" 2>/dev/null || true
    chmod 0640 "$env_file"
    return
  fi
  local database_url
  database_url="$(setup_database)"
  local jwt_secret="${LAGRAN_JWT_SECRET:-$(rand_hex 32)}"
  local agent_token="${LAGRAN_AGENT_TOKEN:-$(rand_hex 24)}"
  # Other hosts (edges, a second DNS) reach the control plane on 8080 directly;
  # the console is proxied by Caddy over loopback either way.
  local default_bind="0.0.0.0:8080"
  if [[ "$CONTROL_EXPOSE" != "1" ]]; then default_bind="127.0.0.1:8080"; fi
  local bind="${LAGRAN_CONTROL_BIND:-$default_bind}"
  CONTROL_PORT="${bind##*:}"
  # Never create the well-known demo account by default: it is incompatible
  # with REQUIRE_SECURE_SECRETS=true below, so the service would refuse to boot.
  local seed_demo="${LAGRAN_SEED_DEMO:-false}"
  # Shared HMAC key for signed WAF rulepacks. Required by both the control plane
  # and every edge; a fresh install must generate it or the control plane
  # exits with "WAF_RULEPACK_SECRET is not set".
  local waf_rulepack_secret="${LAGRAN_WAF_RULEPACK_SECRET:-$(rand_hex 32)}"
  local public_url="${LAGRAN_CONTROL_PUBLIC_URL:-${CONTROL_PUBLIC_URL:-}}"
  # The node wizard puts this URL into edge install commands as CONTROL_URL.
  if [[ -z "$public_url" && "${bind%%:*}" != "127.0.0.1" && -n "$PUBLIC_ADDR" ]]; then
    public_url="http://${PUBLIC_ADDR}:${CONTROL_PORT}"
  fi
  # First boot runs a short bootstrap window: loopback only, registration open
  # and the captcha off, so the installer itself can create the administrator.
  # finish_control_bootstrap() restores the real values before anyone else can
  # reach the port.
  local first_bind="$bind" allow_register=false captcha=true
  if [[ "$CONTROL_BOOTSTRAP" == "1" ]]; then
    first_bind="127.0.0.1:${CONTROL_PORT}"
    allow_register=true
    captcha=false
  fi
  CONTROL_FINAL_BIND="$bind"
  umask 077
  cat > "$env_file" <<EOF
CONTROL_BIND=${first_bind}
CONTROL_PUBLIC_URL=${public_url}
DATABASE_URL=${database_url}
DATABASE_MAX_CONNECTIONS=20
DATABASE_MIN_CONNECTIONS=2
DATABASE_ACQUIRE_TIMEOUT_SECS=5
JWT_SECRET=${jwt_secret}
SSH_GRANT_KEY=$(rand_hex 32)
EDGE_CHALLENGE_SECRET=${CHALLENGE_SECRET:-$(rand_hex 32)}
REQUIRE_SECURE_SECRETS=true
WAF_RULEPACK_SECRET=${waf_rulepack_secret}
AGENT_TOKEN=${agent_token}
SEED_DEMO=${seed_demo}
ALLOW_REGISTER=${allow_register}
CAPTCHA_ENABLED=${captcha}
REDIS_URL=${LAGRAN_REDIS_URL:-}
REDIS_SESSION_TTL_SECS=30
CONFIG_CACHE_TTL_SECS=3
LOG_RETENTION_DAYS=14
ALERT_EVAL_SECS=30
LAGRAN_UPDATE_REPO=${REPO}
LAGRAN_GITHUB_TOKEN=${GH_TOKEN}
LAGRAN_UPDATE_HELPER=${BIN_DIR}/lagran-update
LAGRAN_UPDATE_REQUEST_PATH=${DATA_DIR}/update.request
LAGRAN_UPDATE_STATUS_PATH=${DATA_DIR}/update.status
GEOIP_COUNTRY_DB=${geoip_db}
EOF
  chown root:lagran-control "$env_file"
  chmod 0640 "$env_file"
  umask 022
  if [[ "$seed_demo" == "true" ]]; then
    warn "SEED_DEMO=true creates demo@lagran.dev / demo123; change it immediately"
  fi
}

# Register the first administrator (and, when an edge is installed here too,
# its node) through the API while the bootstrap window is open.
bootstrap_control() {
  local base="http://127.0.0.1:${CONTROL_PORT}"
  log "Creating administrator ${ADMIN_EMAIL}"
  local body resp role
  body="{\"email\":\"$(json_escape "$ADMIN_EMAIL")\",\"password\":\"$(json_escape "$ADMIN_PASSWORD")\",\"name\":\"admin\"}"
  if resp="$(curl -sS -X POST "$base/api/v1/auth/register" -H 'Content-Type: application/json' --data-binary "$body" 2>&1)"; then
    ADMIN_JWT="$(printf '%s' "$resp" | json_field token)"
    role="$(printf '%s' "$resp" | json_field role)"
  fi
  if [[ -z "$ADMIN_JWT" ]]; then
    warn "administrator was not created: ${resp:-no response}"
  elif [[ "$role" != "admin" ]]; then
    # The database already had accounts (reused database), so the new account
    # is a plain user and cannot register nodes.
    warn "the database already has users; ${ADMIN_EMAIL} was created as a normal user"
    ADMIN_JWT=""
    ADMIN_CREATED=1
  else
    ADMIN_CREATED=2
    # Save the generated password right away: if a later step fails, the
    # summary at the end never prints and the password would be lost.
    if [[ "$ADMIN_GENERATED" == "1" ]]; then
      (umask 077; printf '控制台管理员\n账号：%s\n密码：%s\n（登录后请修改密码并删除本文件）\n' \
        "$ADMIN_EMAIL" "$ADMIN_PASSWORD" > "$CONF_DIR/admin-credentials.txt")
    fi
  fi

  if [[ -n "$ADMIN_JWT" ]] && has_component edge && [[ ! -f "$CONF_DIR/edge.env" ]]; then
    log "Registering local edge node ${EDGE_NODE_NAME}"
    body="{\"name\":\"$(json_escape "$EDGE_NODE_NAME")\",\"region\":\"$(json_escape "${NODE_REGION:-local}")\",\"agent_url\":\"http://127.0.0.1:8082\",\"public_ip\":\"$(json_escape "$PUBLIC_ADDR")\",\"weight\":100}"
    resp="$(curl -sS -X POST "$base/api/v1/nodes" -H 'Content-Type: application/json' \
      -H "Authorization: Bearer ${ADMIN_JWT}" --data-binary "$body" 2>&1 || true)"
    EDGE_NODE_TOKEN="$(printf '%s' "$resp" | sed -n 's/.*"token"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -1)"
    if [[ -n "$EDGE_NODE_TOKEN" ]]; then
      # Persist the node config now; the token is only returned once, and a
      # re-run after a later failure must still find it.
      write_edge_env
    else
      warn "edge node was not registered: ${resp}"
    fi
  fi
}

finish_control_bootstrap() {
  local env_file="$CONF_DIR/control.env"
  set_env_value "$env_file" CONTROL_BIND "$CONTROL_FINAL_BIND"
  set_env_value "$env_file" ALLOW_REGISTER false
  set_env_value "$env_file" CAPTCHA_ENABLED true
  systemctl restart lagran-control
}

write_control_unit() {
  cat > /etc/systemd/system/lagran-control.service <<EOF
[Unit]
Description=Lagran CDN Control Plane
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
Type=simple
User=lagran-control
Group=lagran
EnvironmentFile=${CONF_DIR}/control.env
# Repair the shared data directory before dropping privileges. Upgrades from
# the legacy single-user layout can leave it root:lagran 0755, where the split
# lagran-control account cannot create update.request. The + prefix runs these
# helpers as root even though User= is set below.
ExecStartPre=+/bin/mkdir -p ${DATA_DIR}
ExecStartPre=+/bin/chgrp lagran ${DATA_DIR}
ExecStartPre=+/bin/chmod g+rwxs ${DATA_DIR}
ExecStart=${BIN_DIR}/lagran-control
WorkingDirectory=${DATA_DIR}
Restart=always
RestartSec=3
LimitNOFILE=65535
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF
}

install_update_helper() {
  [[ -f "$STAGE/update.sh" ]] || return 0
  if [[ -f "$BIN_DIR/lagran-update" ]] && ! grep -q 'updater-schema: 2' "$STAGE/update.sh"; then
    warn "release contains an older update helper; keeping the installed helper"
  else
    install -m 0755 "$STAGE/update.sh" "$BIN_DIR/lagran-update"
  fi
  cat > "$CONF_DIR/update.env" <<EOF
LAGRAN_PREFIX=${PREFIX}
LAGRAN_DATA_DIR=${DATA_DIR}
LAGRAN_UPDATE_REPO=${REPO}
LAGRAN_GITHUB_TOKEN=${GH_TOKEN}
LAGRAN_UPDATE_REQUEST_PATH=${DATA_DIR}/update.request
LAGRAN_UPDATE_STATUS_PATH=${DATA_DIR}/update.status
EOF
  chmod 0600 "$CONF_DIR/update.env"
  cat > /etc/systemd/system/lagran-update.service <<EOF
[Unit]
Description=Lagran CDN privileged updater
After=network-online.target
Wants=network-online.target

[Service]
Type=oneshot
ExecStart=${BIN_DIR}/lagran-update
TimeoutStartSec=600
EOF
  cat > /etc/systemd/system/lagran-update.path <<EOF
[Unit]
Description=Watch for Lagran CDN update requests

[Path]
PathExists=${DATA_DIR}/update.request
Unit=lagran-update.service

[Install]
WantedBy=multi-user.target
EOF
  if command -v systemctl >/dev/null 2>&1; then
    systemctl daemon-reload
    if [[ "$START_SERVICE" == "1" ]]; then
      systemctl enable --now lagran-update.path
    else
      systemctl enable lagran-update.path
    fi
  fi
}

install_edge_update_helper() {
  if [[ ! -f "$STAGE/edge-update.sh" ]]; then
    warn "release has no edge-update.sh; console edge upgrade will be unavailable"
    return 0
  fi
  install -m 0755 "$STAGE/edge-update.sh" "$BIN_DIR/lagran-edge-update"
  cat > "$CONF_DIR/edge-update.env" <<EOF
LAGRAN_PREFIX=${PREFIX}
LAGRAN_DATA_DIR=${DATA_DIR}
LAGRAN_UPDATE_REPO=${REPO}
LAGRAN_GITHUB_TOKEN=${GH_TOKEN}
EDGE_UPDATE_REQUEST_PATH=${DATA_DIR}/edge-update.request
EDGE_UPDATE_STATUS_PATH=${DATA_DIR}/edge-update.status
EOF
  chmod 0600 "$CONF_DIR/edge-update.env"
  cat > /etc/systemd/system/lagran-edge-update.service <<EOF
[Unit]
Description=Lagran CDN edge privileged updater
After=network-online.target
Wants=network-online.target

[Service]
Type=oneshot
ExecStart=${BIN_DIR}/lagran-edge-update
TimeoutStartSec=600
EOF
  cat > /etc/systemd/system/lagran-edge-update.path <<EOF
[Unit]
Description=Watch for Lagran CDN edge update requests

[Path]
PathExists=${DATA_DIR}/edge-update.request
Unit=lagran-edge-update.service

[Install]
WantedBy=multi-user.target
EOF
  if command -v systemctl >/dev/null 2>&1; then
    systemctl daemon-reload
    if [[ "$START_SERVICE" == "1" ]]; then
      systemctl enable --now lagran-edge-update.path
    else
      systemctl enable lagran-edge-update.path
    fi
  fi
}

write_edge_env() {
  local env_file="$CONF_DIR/edge.env"
  if [[ -f "$env_file" ]]; then
    log "Keeping existing $env_file"
    chown root:lagran-edge "$env_file" 2>/dev/null || true
    chmod 0640 "$env_file"
    return
  fi
  local token="${EDGE_NODE_TOKEN:-${AGENT_TOKEN:-}}"
  [[ -n "$token" ]] || die "edge install requires AGENT_TOKEN (create a node in the console first)"
  local node_id="${EDGE_NODE_NAME:-${NODE_ID:-$(hostname -s)}}"
  local control_url="${CONTROL_URL:-http://127.0.0.1:${CONTROL_PORT}}"
  [[ "$CONTROL_BOOTSTRAP" == "1" ]] && control_url="http://127.0.0.1:${CONTROL_PORT}"

  # A node created in the console wizard has a rendered edge.env stored on the
  # control plane; prefer it so the hardware sizing chosen there is kept.
  if [[ "$CONTROL_BOOTSTRAP" != "1" ]]; then
    local fetched="$CONF_DIR/.edge.env.download"
    if (umask 077; curl -fsS --max-time 10 -H "X-Lagran-Agent-Token: ${token}" \
        -o "$fetched" "${control_url%/}/api/v1/edge/config?edge=${node_id}") 2>/dev/null \
      && grep -q '^AGENT_TOKEN=' "$fetched"; then
      log "Using edge.env generated by the console for ${node_id}"
      mv "$fetched" "$env_file"
      chown root:lagran-edge "$env_file"
      chmod 0640 "$env_file"
      return
    fi
    rm -f "$fetched"
  fi

  local edge_listen="${EDGE_LISTEN:-0.0.0.0:80}"
  local https_listen="${EDGE_HTTPS_LISTEN-0.0.0.0:443}"
  local admin_listen="${EDGE_ADMIN_LISTEN:-127.0.0.1:8082}"
  local agent_url="${AGENT_URL:-http://127.0.0.1:8082}"
  if [[ "$CONTROL_BOOTSTRAP" != "1" && "$agent_url" == http://127.0.0.1:* ]]; then
    case "$control_url" in
      http://127.0.0.1*|http://localhost*) ;;
      *) warn "AGENT_URL=${agent_url} only works when the control plane runs on this host; set AGENT_URL and EDGE_ADMIN_LISTEN for a remote control plane" ;;
    esac
  fi
  # Clearance cookies and challenge nonces must be signed with the same key on
  # every edge. Generate one per install; multi-edge deployments should pass the
  # same CHALLENGE_SECRET to each installer.
  local challenge_secret="${CHALLENGE_SECRET:-}"
  if [[ -z "$challenge_secret" && -f "$CONF_DIR/control.env" ]]; then
    challenge_secret="$(env_value "$CONF_DIR/control.env" EDGE_CHALLENGE_SECRET)"
  fi
  challenge_secret="${challenge_secret:-$(rand_hex 32)}"
  # Signed WAF rulepacks need the same key as the control plane. Reuse the one
  # written to control.env when this host also runs the control plane.
  local waf_rulepack_secret="${WAF_RULEPACK_SECRET:-}"
  if [[ -z "$waf_rulepack_secret" && -f "$CONF_DIR/control.env" ]]; then
    waf_rulepack_secret="$(sed -n 's/^WAF_RULEPACK_SECRET=//p' "$CONF_DIR/control.env" | tail -1)"
  fi
  [[ -n "$waf_rulepack_secret" ]] || warn "WAF_RULEPACK_SECRET is empty; edges will reject signed WAF rulepacks"
  umask 077
  cat > "$env_file" <<EOF
EDGE_LISTEN=${edge_listen}
EDGE_HTTPS_LISTEN=${https_listen}
EDGE_ADMIN_LISTEN=${admin_listen}
NODE_ID=${node_id}
NODE_REGION=${NODE_REGION:-local}
PUBLIC_IP=${PUBLIC_ADDR}
CONTROL_URL=${control_url}
AGENT_URL=${agent_url}
AGENT_TOKEN=${token}
WAF_RULEPACK_SECRET=${waf_rulepack_secret}
CACHE_BACKEND=memory
CACHE_TTL=60
REDIS_URL=${REDIS_URL:-}
CC_REDIS_PREFIX=${CC_REDIS_PREFIX:-lagran:cc}
CC_REDIS_TIMEOUT_MS=${CC_REDIS_TIMEOUT_MS:-50}
RATE_LIMIT_REDIS_PREFIX=${RATE_LIMIT_REDIS_PREFIX:-lagran:rate}
CC_STATIC_MULTIPLIER=${CC_STATIC_MULTIPLIER:-8}
CC_AUTH_DIVISOR=${CC_AUTH_DIVISOR:-2}
CC_DOWNLOAD_MULTIPLIER=${CC_DOWNLOAD_MULTIPLIER:-2}
CC_BEHAVIOR_ENABLED=${CC_BEHAVIOR_ENABLED:-false}
CC_BEHAVIOR_SCORE=${CC_BEHAVIOR_SCORE:-4}
LARGE_CACHE_ENABLED=${LARGE_CACHE_ENABLED:-false}
LARGE_CACHE_DIR=${LARGE_CACHE_DIR:-${DATA_DIR}/large-cache}
LARGE_CACHE_MIN_OBJECT_BYTES=${LARGE_CACHE_MIN_OBJECT_BYTES:-8388608}
LARGE_CACHE_MAX_OBJECT_BYTES=${LARGE_CACHE_MAX_OBJECT_BYTES:-1073741824}
LARGE_CACHE_FILL_MAX_BYTES=${LARGE_CACHE_FILL_MAX_BYTES:-0}
LARGE_CACHE_CHUNK_BYTES=${LARGE_CACHE_CHUNK_BYTES:-1048576}
LARGE_CACHE_FILL_QUEUE_BYTES=${LARGE_CACHE_FILL_QUEUE_BYTES:-134217728}
LARGE_CACHE_FILL_QUEUE_TOTAL_BYTES=${LARGE_CACHE_FILL_QUEUE_TOTAL_BYTES:-268435456}
LARGE_CACHE_MIN_FREE_BYTES=${LARGE_CACHE_MIN_FREE_BYTES:-1073741824}
LARGE_CACHE_MAX_BYTES=${LARGE_CACHE_MAX_BYTES:-21474836480}
CHALLENGE_SECRET=${challenge_secret}
EDGE_UPDATE_HELPER=${BIN_DIR}/lagran-edge-update
EDGE_UPDATE_REQUEST_PATH=${DATA_DIR}/edge-update.request
EOF
  chown root:lagran-edge "$env_file"
  chmod 0640 "$env_file"
  umask 022
}

write_edge_unit() {
  cat > /etc/systemd/system/lagran-edge.service <<EOF
[Unit]
Description=Lagran CDN Edge Node
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=lagran-edge
Group=lagran
EnvironmentFile=${CONF_DIR}/edge.env
# The edge writes cache/cert files and (when asked by the control plane) an
# edge-update.request inside the shared data directory. Repair the top-level
# mode on every start so a legacy 0755 root:lagran directory does not block it.
ExecStartPre=+/bin/mkdir -p ${DATA_DIR}
ExecStartPre=+/bin/chgrp lagran ${DATA_DIR}
ExecStartPre=+/bin/chmod g+rwxs ${DATA_DIR}
ExecStart=${BIN_DIR}/lagran-edge
WorkingDirectory=${DATA_DIR}
Restart=always
RestartSec=3
LimitNOFILE=1048576
AmbientCapabilities=CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_BIND_SERVICE
# Hardening shared with the control unit. UMask matters most here: the edge
# writes TLS private keys to disk, and without it they land as 0644.
UMask=0077
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=${DATA_DIR} ${LOG_DIR}

[Install]
WantedBy=multi-user.target
EOF
}

write_dns_env() {
  local env_file="$CONF_DIR/dns.env"
  if [[ -f "$env_file" ]]; then
    log "Keeping existing $env_file"
    # Older installs left this out, so the node sent no heartbeat and the
    # console could not show its version or whether it has the latest config.
    if [[ -z "$(env_value "$env_file" DNS_PUBLIC_IP)" && -n "$PUBLIC_ADDR" ]]; then
      set_env_value "$env_file" DNS_PUBLIC_IP "$PUBLIC_ADDR"
    fi
    chown root:lagran-dns "$env_file" 2>/dev/null || true
    chmod 0640 "$env_file"
    return
  fi
  local agent_token="$DNS_AGENT_TOKEN"
  local control_url="${CONTROL_URL:-}"
  if [[ -f "$CONF_DIR/control.env" ]]; then
    # Control plane on this host: always use its global token over loopback.
    agent_token="$(env_value "$CONF_DIR/control.env" AGENT_TOKEN)"
    control_url="http://127.0.0.1:${CONTROL_PORT}"
  elif [[ -z "$agent_token" ]] && ! has_component edge; then
    agent_token="${AGENT_TOKEN:-}"
  fi
  if [[ -n "$control_url" && -z "$agent_token" ]]; then
    warn "AGENT_TOKEN is empty; lagran-dns cannot pull its zones from the control plane"
  fi
  local zone_lines
  if [[ -n "${DNS_ZONE:-}" ]]; then
    zone_lines="# Fixed zone for running without the console; a CDN 主域 saved in the console replaces it.
DNS_ZONE=${DNS_ZONE}
DNS_NS_NAME=${DNS_NS_NAME:-ns1.${DNS_ZONE}}
DNS_NS_IP=${DNS_NS_IP:-${PUBLIC_ADDR:-127.0.0.1}}"
  else
    zone_lines="# CDN 主域与 DNS 服务器在控制台「智能 DNS → CDN 主域」配置，每 DNS_SYNC_SECS 秒自动同步。
# 只有不接控制面、单独运行时才在这里写死：
# DNS_ZONE=edge.example.com
# DNS_NS_NAME=ns1.edge.example.com
# DNS_NS_IP=203.0.113.53"
  fi
  umask 077
  # lagran-dns only talks plain HTTP to a control plane on this machine unless
  # told otherwise; a second NS server reaching control over HTTP :8080 needs it.
  local insecure=false
  case "$control_url" in
    http://127.*|http://localhost*|http://\[::1\]*) ;;
    http://*)
      insecure=true
      warn "DNS reaches the control plane over plain HTTP ($control_url); the agent token travels unencrypted. Prefer https:// on untrusted networks."
      ;;
  esac
  cat > "$env_file" <<EOF
DNS_LISTEN=${DNS_LISTEN:-0.0.0.0:53}
DNS_CONTROL_INSECURE=${insecure}
${zone_lines}
DNS_TTL=${DNS_TTL:-30}
DNS_TCP=${DNS_TCP:-true}
DNS_WORKERS=${DNS_WORKERS:-8}
CONTROL_URL=${control_url}
AGENT_TOKEN=${agent_token}
DNS_SYNC_SECS=${DNS_SYNC_SECS:-15}
# Sent with each heartbeat; the console matches this node by it.
DNS_PUBLIC_IP=${PUBLIC_ADDR}
DNS_CACHE_SECS=${DNS_CACHE_SECS:-30}
DNS_STALE_SECS=${DNS_STALE_SECS:-300}
DNS_CONTROL_TIMEOUT_MS=${DNS_CONTROL_TIMEOUT_MS:-800}
DNS_FALLBACK_IP=${DNS_FALLBACK_IP:-}
EOF
  chown root:lagran-dns "$env_file"
  chmod 0640 "$env_file"
  umask 022
}

write_dns_unit() {
  cat > /etc/systemd/system/lagran-dns.service <<EOF
[Unit]
Description=Lagran CDN Authoritative DNS
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=lagran-dns
Group=lagran
EnvironmentFile=${CONF_DIR}/dns.env
ExecStart=${BIN_DIR}/lagran-dns
WorkingDirectory=${DATA_DIR}
Restart=always
RestartSec=3
LimitNOFILE=65535
AmbientCapabilities=CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_BIND_SERVICE
UMask=0077
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=${DATA_DIR} ${LOG_DIR}

[Install]
WantedBy=multi-user.target
EOF
}

# Fail loudly if the SPA directory is not readable by a non-root user.
#
# The reverse proxy runs as its own account, and `file_server` answers 403 rather
# than 404 when it cannot traverse the root — which is how an upgrade made the
# console unreachable while the control plane stayed healthy. Checking from root
# would not catch it, so the read is attempted as `nobody`.
check_web_readable() {
  local dir="$1"
  local index="${dir}/index.html"
  [[ -f "$index" ]] || die "web root is missing ${index}"
  # Probe as uid 65534 if this process can drop privileges and setpriv exists.
  # When it cannot, skip rather than fail: a missing probe tool must not abort an
  # otherwise correct install (the earlier draft treated "setpriv unavailable" as
  # "not readable" and would have blocked every non-root run).
  if ! command -v setpriv >/dev/null 2>&1 \
     || ! setpriv --reuid=65534 --regid=65534 --clear-groups true 2>/dev/null; then
    warn "cannot drop privileges to verify ${dir}; skipping the readability check"
    return 0
  fi
  if ! setpriv --reuid=65534 --regid=65534 --clear-groups test -r "$index" 2>/dev/null; then
    die "web root ${dir} is not readable by an unprivileged user; the reverse proxy would answer 403 (fix: chmod -R a+rX ${dir})"
  fi
  if ! setpriv --reuid=65534 --regid=65534 --clear-groups test -x "$dir" 2>/dev/null; then
    die "web root ${dir} is not traversable by an unprivileged user; the reverse proxy would answer 403 (fix: chmod a+x ${dir})"
  fi
}

# Hostnames/IPs the console certificate is issued for. Browsers send no SNI
# for IP literals, so the first entry also becomes Caddy's default_sni.
console_hosts() {
  local hosts=() local_ip=""
  [[ -n "$CONSOLE_DOMAIN" ]] && hosts+=("$CONSOLE_DOMAIN")
  [[ -n "$PUBLIC_ADDR" ]] && hosts+=("$PUBLIC_ADDR")
  if command -v ip >/dev/null 2>&1; then
    local_ip="$(ip -4 route get 1.1.1.1 2>/dev/null | sed -n 's/.* src \([0-9.]*\).*/\1/p' | head -1 || true)"
  fi
  # Behind NAT the interface address differs from the public one; keep both so
  # the console also works from the private network.
  if [[ -n "$local_ip" && "$local_ip" != "$PUBLIC_ADDR" ]]; then hosts+=("$local_ip"); fi
  hosts+=("127.0.0.1")
  printf '%s\n' "${hosts[@]}"
}

CADDY_MARKER="# Managed by the Lagran installer"

configure_caddy() {
  local upstream="${CONSOLE_UPSTREAM:-127.0.0.1:${CONTROL_PORT}}"
  local site_file=/etc/caddy/lagran.caddy main_file=/etc/caddy/Caddyfile
  local hosts=() h addresses="" first=""
  mapfile -t hosts < <(console_hosts)
  first="${hosts[0]}"
  for h in "${hosts[@]}"; do
    if [[ "$CONSOLE_TLS" == "internal" ]]; then
      addresses+="${addresses:+, }https://${h}:${CONSOLE_PORT}"
    fi
  done
  [[ "$CONSOLE_TLS" == "off" ]] && addresses="http://:${CONSOLE_PORT}"

  install -d -m 0755 /etc/caddy
  {
    printf '%s. Re-running install.sh rewrites this file.\n' "$CADDY_MARKER"
    printf '%s {\n' "$addresses"
    [[ "$CONSOLE_TLS" == "internal" ]] && printf '    tls internal\n'
    cat <<EOF
    root * ${WEB_DIR}
    encode gzip
    @api path /api/* /health /ready
    handle @api {
        reverse_proxy ${upstream}
    }
    handle {
        try_files {path} /index.html
        file_server
    }
}
EOF
  } > "$site_file"
  chmod 0644 "$site_file"

  # Own the main Caddyfile when it is absent, ours, or the packaged sample
  # (which serves :80 and would collide with the edge). A hand-written one is
  # left alone apart from the import line.
  if [[ ! -f "$main_file" ]] || grep -qF "$CADDY_MARKER" "$main_file" \
    || grep -q '/usr/share/caddy' "$main_file"; then
    if [[ -f "$main_file" ]] && ! grep -qF "$CADDY_MARKER" "$main_file"; then
      cp -p "$main_file" "${main_file}.lagran-backup"
    fi
    cat > "$main_file" <<EOF
${CADDY_MARKER}. Original (if any): ${main_file}.lagran-backup
{
    # Ports 80/443 belong to the Lagran edge: no HTTP->HTTPS redirect listener,
    # no public ACME, certificates come from Caddy's internal CA.
    auto_https disable_redirects
    skip_install_trust
    default_sni ${first}
}

import ${site_file}
EOF
    chmod 0644 "$main_file"
  elif ! grep -q "import ${site_file}" "$main_file"; then
    printf '\nimport %s\n' "$site_file" >> "$main_file"
    warn "${main_file} has custom sites; added an import line. Make sure none of them listens on :80/:443 if the edge runs here."
  fi
}

start_caddy() {
  caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile >/dev/null 2>&1 \
    || die "Caddy rejected /etc/caddy/Caddyfile; run: caddy validate --config /etc/caddy/Caddyfile"
  if [[ "$START_SERVICE" == "1" ]] && command -v systemctl >/dev/null 2>&1; then
    systemctl enable caddy >/dev/null 2>&1 || true
    if ! systemctl restart caddy; then
      warn "Caddy failed to start; see: journalctl -u caddy -n 50 --no-pager"
      CONSOLE_FAILED=1
      return 0
    fi
    local scheme=https
    [[ "$CONSOLE_TLS" == "off" ]] && scheme=http
    if ! wait_health_insecure "${scheme}://127.0.0.1:${CONSOLE_PORT}/"; then
      warn "console did not answer on ${scheme}://127.0.0.1:${CONSOLE_PORT}/; see: journalctl -u caddy -n 50"
    fi
  fi
}

wait_health_insecure() {
  local url=$1 _
  for _ in $(seq 1 20); do
    curl -kfsS -o /dev/null "$url" 2>/dev/null && return 0
    sleep 1
  done
  return 1
}

install_frontend() {
  [[ -f "$STAGE/web/index.html" ]] || die "release does not contain web/index.html"
  install -d -m 0755 "$WEB_DIR"
  if command -v rsync >/dev/null 2>&1; then
    rsync -a --delete "$STAGE/web/" "$WEB_DIR/"
  else
    cp -a "$STAGE/web/." "$WEB_DIR/"
  fi
  chmod -R a+rX "$WEB_DIR"
  check_web_readable "$WEB_DIR"
  install_env_example frontend.env.example
}

enable_service() {
  local service=$1
  if [[ "$START_SERVICE" != "1" ]]; then
    log "Installed ${service}.service; start with: systemctl enable --now ${service}"
    return
  fi
  if ! command -v systemctl >/dev/null 2>&1; then
    warn "systemd not available; start ${BIN_DIR}/${service} manually"
    return
  fi
  systemctl daemon-reload
  systemctl enable --now "${service}.service"
  # Pick up a changed env file / binary on re-runs.
  systemctl restart "${service}.service"
}

BOOTSTRAP_OPEN=0
CONSOLE_FAILED=0

main_control() {
  log "Installing control plane"
  if [[ -f "$CONF_DIR/control.env" ]]; then
    local existing
    existing="$(env_value "$CONF_DIR/control.env" CONTROL_BIND)"
    CONTROL_PORT="${existing##*:}"
    CONTROL_PORT="${CONTROL_PORT:-8080}"
  fi
  require_port "$CONTROL_PORT" tcp 'lagran-control' "control plane"
  ensure_postgres
  install_binary lagran-control
  ensure_users
  install_env_example control.env.example
  ensure_geoip_country_db
  write_control_env
  [[ "$CONTROL_BOOTSTRAP" == "1" ]] && BOOTSTRAP_OPEN=1
  write_control_unit
  install_update_helper
  enable_service lagran-control
  if command -v systemctl >/dev/null 2>&1 && [[ "$START_SERVICE" == "1" ]]; then
    wait_health "http://127.0.0.1:${CONTROL_PORT}/health" \
      || die "control health check failed; see: journalctl -u lagran-control -n 50 --no-pager"
    if [[ "$BOOTSTRAP_OPEN" == "1" ]]; then
      bootstrap_control
      finish_control_bootstrap
      BOOTSTRAP_OPEN=0
      wait_health "http://127.0.0.1:${CONTROL_PORT}/health" \
        || die "control health check failed after bootstrap; see: journalctl -u lagran-control -n 50 --no-pager"
    fi
  fi
  [[ "$(env_value "$CONF_DIR/control.env" CONTROL_BIND)" == 127.0.0.1:* ]] || open_port "${CONTROL_PORT}/tcp"
  log "Control plane ready on port ${CONTROL_PORT}"
}

main_frontend() {
  log "Installing admin console"
  install_frontend
  if [[ "$CONFIGURE_CADDY" != "1" ]]; then
    log "Console files are in ${WEB_DIR}; serve them and proxy /api/*, /health, /ready to the control plane"
    return
  fi
  require_port "$CONSOLE_PORT" tcp 'caddy' "console"
  # Write the config before the package exists: the Debian package starts
  # Caddy right away, and its sample config would grab port 80.
  configure_caddy
  ensure_caddy
  start_caddy
  open_port "${CONSOLE_PORT}/tcp"
}

main_dns() {
  log "Installing authoritative DNS"
  local listen="${DNS_LISTEN:-0.0.0.0:53}"
  if [[ "$listen" == 0.0.0.0:53 || "$listen" == "[::]:53" ]]; then
    free_dns_port
    require_port 53 udp 'lagran-dns' "DNS"
    require_port 53 tcp 'lagran-dns' "DNS"
  fi
  install_binary lagran-dns
  ensure_users
  install_env_example dns.env.example
  write_dns_env
  write_dns_unit
  # Start even before the console has a CDN 主域: lagran-dns answers REFUSED
  # until a sync brings a zone, then serves it without a restart.
  enable_service lagran-dns
  open_port 53/tcp
  open_port 53/udp
}

EDGE_SKIPPED=0

main_edge() {
  log "Installing edge node"
  if [[ ! -f "$CONF_DIR/edge.env" && -z "${EDGE_NODE_TOKEN:-${AGENT_TOKEN:-}}" ]]; then
    warn "no node token for the edge; create the node in the console and re-run: install.sh edge"
    EDGE_SKIPPED=1
    return
  fi
  require_port 80 tcp 'lagran-edge' "edge HTTP"
  require_port 443 tcp 'lagran-edge' "edge HTTPS"
  install_binary lagran-edge
  ensure_users
  install_env_example edge.env.example
  write_edge_env
  write_edge_unit
  install_edge_update_helper
  enable_service lagran-edge
  open_port 80/tcp
  open_port 443/tcp
}

print_summary() {
  local out=() scheme=https h
  [[ "$CONSOLE_TLS" == "off" ]] && scheme=http
  out+=("" "================ Lagran CDN 安装完成 ================")
  if has_component control; then
    out+=("控制面      http://127.0.0.1:${CONTROL_PORT}   配置 ${CONF_DIR}/control.env")
    if [[ "$(env_value "$CONF_DIR/control.env" CONTROL_BIND)" != 127.0.0.1:* && -n "$PUBLIC_ADDR" ]]; then
      out+=("            其他机器的节点/DNS 使用 CONTROL_URL=http://${PUBLIC_ADDR}:${CONTROL_PORT}")
    fi
    out+=("            全局 AGENT_TOKEN（DNS 用）见 ${CONF_DIR}/control.env")
  fi
  if has_component frontend && [[ "$CONFIGURE_CADDY" == "1" ]]; then
    out+=("管理控制台")
    while IFS= read -r h; do
      [[ "$h" == 127.0.0.1 ]] && continue
      out+=("            ${scheme}://${h}:${CONSOLE_PORT}")
    done < <(console_hosts)
    [[ "$scheme" == https ]] && out+=("            （自签证书，浏览器首次访问需手动信任）")
    [[ "$CONSOLE_FAILED" == "1" ]] && out+=("            ⚠ Caddy 启动失败：journalctl -u caddy -n 50 --no-pager")
  fi
  case "$ADMIN_CREATED" in
    2)
      out+=("管理员      ${ADMIN_EMAIL}")
      if [[ "$ADMIN_GENERATED" == "1" ]]; then
        out+=("            密码 ${ADMIN_PASSWORD}   （自动生成，请登录后立即修改）")
      fi
      ;;
    1) out+=("管理员      未创建：数据库里已有账号，请用原管理员登录") ;;
    *)
      if [[ "$CONTROL_BOOTSTRAP" == "1" ]]; then
        out+=("管理员      创建失败：临时设置 ALLOW_REGISTER=true 后在控制台注册，第一个账号即管理员")
      fi
      ;;
  esac
  if has_component dns; then
    local zone
    zone="$(env_value "$CONF_DIR/dns.env" DNS_ZONE)"
    if [[ "$START_SERVICE" != "1" ]]; then
      out+=("权威 DNS    已安装未启动：systemctl enable --now lagran-dns")
    elif systemctl is-active --quiet lagran-dns 2>/dev/null; then
      out+=("权威 DNS    已启动  配置 ${CONF_DIR}/dns.env")
    else
      out+=("权威 DNS    ⚠ 启动失败：journalctl -u lagran-dns -n 50 --no-pager")
    fi
    if [[ -n "$zone" ]]; then
      out+=("            zone ${zone} 来自 dns.env；控制台保存 CDN 主域后以控制台为准")
    else
      out+=("            到控制台「智能 DNS → CDN 主域」填写主域和 DNS 服务器，约 15 秒后自动生效")
    fi
  fi
  if has_component edge; then
    if [[ "$EDGE_SKIPPED" == "1" ]]; then
      out+=("边缘节点    未安装：缺少节点令牌，在控制台创建节点后执行 install.sh edge")
    else
      out+=("边缘节点    $(env_value "$CONF_DIR/edge.env" NODE_ID)  监听 :80 / :443  配置 ${CONF_DIR}/edge.env")
    fi
  fi
  if [[ ${#FIREWALL_PORTS[@]} -gt 0 ]]; then
    out+=("端口        ${FIREWALL_PORTS[*]}（云服务器还需在安全组放行）")
  fi
  out+=("=====================================================")
  printf '%s\n' "${out[@]}"
  if [[ "$ADMIN_GENERATED" == "1" && "$ADMIN_CREATED" == "2" ]]; then
    printf '管理员账号密码另存于 %s（root 可读，修改密码后请删除）\n' "$CONF_DIR/admin-credentials.txt"
  fi
}

cleanup() {
  # Never leave the bootstrap window (open registration, no captcha) behind.
  if [[ "$BOOTSTRAP_OPEN" == "1" && -f "$CONF_DIR/control.env" ]]; then
    BOOTSTRAP_OPEN=0
    finish_control_bootstrap >/dev/null 2>&1 || true
  fi
  rm -rf "${TMP:-}"
}

main() {
  # Files for other service accounts (Caddyfile, web root) must be readable;
  # secret env files tighten the mask locally while they are written.
  umask 022
  parse_args "$@"
  [[ "$EUID" -eq 0 ]] || die "run as root (use sudo)"
  command -v curl >/dev/null 2>&1 || die "curl is required"
  command -v tar >/dev/null 2>&1 || die "tar is required"
  if ! command -v systemctl >/dev/null 2>&1; then
    warn "systemd not found: services will be installed but not started"
    START_SERVICE=0
  fi

  # Re-runs without the token keep the one an earlier install stored.
  if [[ -z "$GH_TOKEN" ]]; then
    local f
    for f in "$CONF_DIR/update.env" "$CONF_DIR/edge-update.env" "$CONF_DIR/control.env"; do
      [[ -f "$f" ]] && GH_TOKEN="$(env_value "$f" LAGRAN_GITHUB_TOKEN)"
      [[ -n "$GH_TOKEN" ]] && break
    done
  fi

  if [[ ${#COMPONENTS[@]} -eq 0 ]]; then
    if tty_available; then
      INTERACTIVE=1
      choose_components
    else
      usage
      exit 1
    fi
  fi
  if [[ -f "$CONF_DIR/control.env" ]]; then
    local existing
    existing="$(env_value "$CONF_DIR/control.env" CONTROL_BIND)"
    CONTROL_PORT="${existing##*:}"
    CONTROL_PORT="${CONTROL_PORT:-8080}"
  fi
  if control_bootstrapping; then CONTROL_BOOTSTRAP=1; fi
  if has_component frontend && ! has_component control && [[ ! -f "$CONF_DIR/control.env" ]]; then
    ask CONSOLE_UPSTREAM "控制面 API 地址（host:port）" "${CONSOLE_UPSTREAM:-127.0.0.1:8080}"
  fi
  gather_settings
  print_plan

  trap cleanup EXIT
  detect_pkg
  resolve_release
  ensure_dirs
  if [[ -f "$STAGE/LICENSE" ]]; then
    install -m 0644 "$STAGE/LICENSE" "$PREFIX/LICENSE"
  fi
  # Order matters: the console proxies to the control plane, DNS reads its
  # token, and the edge needs the node the control bootstrap registered.
  has_component control && main_control
  has_component frontend && main_frontend
  has_component dns && main_dns
  has_component edge && main_edge
  apply_firewall
  print_summary
}

main "$@"
