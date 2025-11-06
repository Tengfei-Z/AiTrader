#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_DEPLOY_CONFIG="$(cd "${SCRIPT_DIR}/.." && pwd)/config/config.yaml"
DEPLOY_CONFIG_FILE="${DEPLOY_CONFIG_FILE:-${DEFAULT_DEPLOY_CONFIG}}"

if [[ ! -f "${DEPLOY_CONFIG_FILE}" ]]; then
  echo "Deployment config ${DEPLOY_CONFIG_FILE} not found. Set DEPLOY_CONFIG_FILE or create config/config.yaml." >&2
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required to parse ${DEPLOY_CONFIG_FILE}. Please install it before running deploy.sh." >&2
  exit 1
fi

python_eval=$(python3 - "$DEPLOY_CONFIG_FILE" "$SCRIPT_DIR" <<'PY'
import os
import pathlib
import shlex
import sys
import base64

CONFIG_PATH = pathlib.Path(sys.argv[1]).resolve()
SCRIPT_DIR = pathlib.Path(sys.argv[2]).resolve()

try:
    import yaml  # type: ignore
except ModuleNotFoundError:
    sys.stderr.write(
        f"PyYAML is required to parse {CONFIG_PATH}. Install it with 'pip install PyYAML'.\n"
    )
    sys.exit(1)

with CONFIG_PATH.open("r", encoding="utf-8") as fh:
    data = yaml.safe_load(fh) or {}

deployment = data.get("deployment") or {}

def dig(obj, path, default=None):
    cur = obj
    for part in path.split('.'):
        if not isinstance(cur, dict) or part not in cur:
            return default
        cur = cur[part]
    return cur

repo_root = dig(deployment, "paths.repo_root")
if not repo_root:
    repo_root = str(SCRIPT_DIR.parent.resolve())

backend_dir = dig(deployment, "paths.backend_dir") or str(pathlib.Path(repo_root) / "backend")
frontend_dir = dig(deployment, "paths.frontend_dir") or str(pathlib.Path(repo_root) / "frontend")
agent_dir = dig(deployment, "paths.agent_dir") or str(pathlib.Path(repo_root) / "agent")
config_file_path = dig(deployment, "paths.config_file") or str(pathlib.Path(repo_root) / "config" / "config.yaml")

backend_binary = dig(deployment, "backend.binary") or str(pathlib.Path(backend_dir) / "target" / "release" / "api-server")
backend_bind_addr = dig(deployment, "backend.bind_addr") or dig(data, "server.bind") or "127.0.0.1:3000"

domain = dig(deployment, "domain")
domain_aliases = dig(deployment, "domain_aliases") or []
if isinstance(domain_aliases, (list, tuple)):
    extra_server_names = [str(alias).strip() for alias in domain_aliases if isinstance(alias, (str, bytes)) and str(alias).strip()]
else:
    extra_server_names = []

server_names = []
if domain:
    server_names.append(str(domain).strip())
server_names.extend(extra_server_names)
if not server_names:
    server_names.append('_')

site_name = dig(deployment, "nginx.site_name") or "aitrader"
nginx_conf_path = dig(deployment, "nginx.conf_path") or f"/etc/nginx/sites-available/{site_name}.conf"
nginx_enabled_path = dig(deployment, "nginx.enabled_path") or f"/etc/nginx/sites-enabled/{site_name}.conf"
https_port_value = dig(deployment, "nginx.https_port")
https_port = int(https_port_value) if https_port_value is not None else 443
http_redirect_value = dig(deployment, "nginx.http_port")
http_redirect_port = int(http_redirect_value) if http_redirect_value is not None else None
https_port_suffix = ""
if https_port != 443:
    https_port_suffix = f":{https_port}"

static_root = dig(deployment, "static.root") or "/var/www/aitrader/frontend"
static_owner = dig(deployment, "static.owner") or "www-data"
static_group = dig(deployment, "static.group") or "www-data"

runtime_env = deployment.get("runtime_env") or {}

db_url = runtime_env.get("database_url") or dig(data, "db.url") or ""
log_file_path = runtime_env.get("log_file_path") or "/var/log/aitrader/api-server.log"
log_level = runtime_env.get("log_level") or dig(data, "logging.level") or "info"
http_proxy = runtime_env.get("http_proxy") or os.getenv("http_proxy") or os.getenv("HTTP_PROXY")
https_proxy = runtime_env.get("https_proxy") or os.getenv("https_proxy") or os.getenv("HTTPS_PROXY")
if isinstance(log_level, dict):
    log_level = log_level.get("value", "info")

service_name = dig(deployment, "systemd.service_name") or "aitrader-backend.service"
unit_path = dig(deployment, "systemd.unit_path") or f"/etc/systemd/system/{service_name}"

agent_cfg = deployment.get("agent") or {}
agent_python = agent_cfg.get("python") or "python3"
agent_module = agent_cfg.get("module") or "agent.scripts.run_agent"
agent_exec_start = agent_cfg.get("exec_start")
if agent_exec_start:
    agent_exec_cmd = str(agent_exec_start)
else:
    agent_exec_cmd = f"{agent_python} -m {agent_module}"
agent_exec_b64 = base64.b64encode(agent_exec_cmd.encode("utf-8")).decode("ascii")

agent_service_name = agent_cfg.get("service_name") or "aitrader-agent.service"
agent_unit_path = agent_cfg.get("unit_path") or f"/etc/systemd/system/{agent_service_name}"
agent_env_file = agent_cfg.get("env_file")
if agent_env_file:
    agent_env_path = pathlib.Path(agent_env_file)
    if agent_env_path.is_absolute():
        agent_env_file = str(agent_env_path)
    else:
        agent_env_file = str(pathlib.Path(agent_dir) / agent_env_path)

agent_log_file_cfg = agent_cfg.get("log_file")
if agent_log_file_cfg:
    agent_log_path = pathlib.Path(agent_log_file_cfg)
    if agent_log_path.is_absolute():
        agent_log_file = str(agent_log_path)
    else:
        agent_log_file = str(pathlib.Path(repo_root) / agent_log_path)
else:
    agent_log_file = str(pathlib.Path(repo_root) / "log" / "agent.log")

agent_work_dir_cfg = agent_cfg.get("working_dir")
if agent_work_dir_cfg:
    agent_work_dir_path = pathlib.Path(agent_work_dir_cfg)
    if agent_work_dir_path.is_absolute():
        agent_work_dir = str(agent_work_dir_path)
    else:
        agent_work_dir = str(pathlib.Path(repo_root) / agent_work_dir_path)
else:
    agent_work_dir = str(pathlib.Path(repo_root))

assignments = {
    "APP_USER": dig(deployment, "app_user") or os.getenv("USER", "root"),
    "DOMAIN": domain,
    "SSL_CERT_PATH": dig(deployment, "ssl.cert_path"),
    "SSL_KEY_PATH": dig(deployment, "ssl.key_path"),
    "REPO_ROOT": repo_root,
    "BACKEND_DIR": backend_dir,
    "FRONTEND_DIR": frontend_dir,
    "CONFIG_FILE": config_file_path,
    "BACKEND_BINARY": backend_binary,
    "BACKEND_BIND_ADDR": backend_bind_addr,
    "STATIC_ROOT": static_root,
    "STATIC_OWNER": static_owner,
    "STATIC_GROUP": static_group,
    "NGINX_SITE_NAME": site_name,
    "NGINX_SERVER_NAMES": " ".join(server_names),
    "NGINX_CONF_PATH": nginx_conf_path,
    "NGINX_ENABLED_PATH": nginx_enabled_path,
    "NGINX_HTTPS_PORT": https_port,
    "NGINX_HTTPS_PORT_SUFFIX": https_port_suffix,
    "SYSTEMD_SERVICE_NAME": service_name,
    "SYSTEMD_UNIT_PATH": unit_path,
    "DATABASE_URL": db_url,
    "LOG_FILE_PATH": log_file_path,
    "LOG_LEVEL": log_level,
    "AGENT_DIR": agent_dir,
    "AGENT_EXEC_START_B64": agent_exec_b64,
    "AGENT_SERVICE_NAME": agent_service_name,
    "AGENT_SYSTEMD_UNIT_PATH": agent_unit_path,
    "AGENT_ENV_FILE": agent_env_file,
    "AGENT_LOG_FILE": agent_log_file,
    "AGENT_WORK_DIR": agent_work_dir,
}

if http_redirect_port is not None:
    assignments["NGINX_HTTP_REDIRECT_PORT"] = http_redirect_port
if http_proxy:
    assignments["HTTP_PROXY"] = http_proxy
if https_proxy:
    assignments["HTTPS_PROXY"] = https_proxy

missing = []
for required_key in ["APP_USER", "SSL_CERT_PATH", "SSL_KEY_PATH"]:
    if not assignments.get(required_key):
        missing.append(required_key)

if missing:
    sys.stderr.write(
        "Missing required deployment configuration keys in {0}: {1}\n".format(
            CONFIG_PATH, ", ".join(sorted(missing))
        )
    )
    sys.exit(1)

lines = []
for key, value in assignments.items():
    if value is None:
        continue
    if isinstance(value, bool):
        value = "true" if value else "false"
    elif isinstance(value, (int, float)):
        value = str(value)
    lines.append(f"{key}={shlex.quote(str(value))}")

print("\n".join(lines))
PY
)

if [[ -z "${python_eval}" ]]; then
  echo "Failed to derive deployment configuration from ${DEPLOY_CONFIG_FILE}." >&2
  exit 1
fi

eval "${python_eval}"

if [[ -n "${AGENT_EXEC_START_B64:-}" ]]; then
  AGENT_EXEC_START="$(printf '%s' "${AGENT_EXEC_START_B64}" | base64 --decode)"
else
  AGENT_EXEC_START=""
fi
unset AGENT_EXEC_START_B64

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Command '$1' not found. Please install it before running deploy.sh." >&2
    exit 1
  fi
}

ensure_root() {
  if [[ "${EUID}" -ne 0 ]]; then
    echo "This script must be run as root (use sudo)." >&2
    exit 1
  fi
}

validate_config() {
  if ! id "${APP_USER}" >/dev/null 2>&1; then
    echo "User '${APP_USER}' does not exist on this system." >&2
    exit 1
  fi

  if [[ ! -d "${BACKEND_DIR}" ]]; then
    echo "Backend directory ${BACKEND_DIR} not found." >&2
    exit 1
  fi

  if [[ ! -d "${AGENT_DIR}" ]]; then
    echo "Agent directory ${AGENT_DIR} not found." >&2
    exit 1
  fi

  if [[ ! -d "${FRONTEND_DIR}" ]]; then
    echo "Frontend directory ${FRONTEND_DIR} not found." >&2
    exit 1
  fi

  if [[ -z "${AGENT_EXEC_START}" ]]; then
    echo "Agent ExecStart command is empty after configuration." >&2
    exit 1
  fi

  local agent_cmd
  agent_cmd="$(printf '%s' "${AGENT_EXEC_START}" | awk '{print $1}')"
  if [[ -z "${agent_cmd}" ]] || ! command -v "${agent_cmd}" >/dev/null 2>&1; then
    echo "Agent executable '${agent_cmd}' not found in PATH." >&2
    exit 1
  fi

  if [[ ! -f "${CONFIG_FILE}" ]]; then
    echo "Config file ${CONFIG_FILE} not found." >&2
    exit 1
  fi

  if [[ ! -f "${SSL_CERT_PATH}" ]]; then
    echo "SSL certificate not found at ${SSL_CERT_PATH}." >&2
    exit 1
  fi

  if [[ ! -f "${SSL_KEY_PATH}" ]]; then
    echo "SSL key not found at ${SSL_KEY_PATH}." >&2
    exit 1
  fi
}

ensure_paths() {
  mkdir -p "${STATIC_ROOT}"
  mkdir -p "$(dirname "${LOG_FILE_PATH}")"
  touch "${LOG_FILE_PATH}"
  chown "${APP_USER}:${APP_USER}" "${LOG_FILE_PATH}"

  if [[ -n "${AGENT_LOG_FILE:-}" ]]; then
    mkdir -p "$(dirname "${AGENT_LOG_FILE}")"
    touch "${AGENT_LOG_FILE}"
    chown "${APP_USER}:${APP_USER}" "${AGENT_LOG_FILE}"
  fi

  adjust_static_permissions
}

adjust_static_permissions() {
  if id "${STATIC_OWNER}" >/dev/null 2>&1 && getent group "${STATIC_GROUP}" >/dev/null 2>&1; then
    chown -R "${STATIC_OWNER}:${STATIC_GROUP}" "${STATIC_ROOT}"
  fi
}

ensure_backend_artifact() {
  echo "[1/7] Checking backend artifact..."
  if [[ ! -x "${BACKEND_BINARY}" ]]; then
    cat >&2 <<EOF
Backend binary not found at ${BACKEND_BINARY}.
Please run 'cargo build --release' as ${APP_USER} before executing deploy.sh.
EOF
    exit 1
  fi
}

ensure_frontend_build() {
  echo "[2/7] Checking frontend build output..."
  local dist_dir="${FRONTEND_DIR}/dist"
  if [[ ! -d "${dist_dir}" ]]; then
    cat >&2 <<EOF
Frontend dist directory not found at ${dist_dir}.
Please run 'npm install' and 'npm run build' inside ${FRONTEND_DIR} before executing deploy.sh.
EOF
    exit 1
  fi
}

sync_static_assets() {
  echo "[3/7] Syncing frontend assets to ${STATIC_ROOT}..."
  local dist_dir="${FRONTEND_DIR}/dist"
  if [[ ! -d "${dist_dir}" ]]; then
    echo "Frontend dist directory not found at ${dist_dir}. Build step may have failed." >&2
    exit 1
  fi

  if command -v rsync >/dev/null 2>&1; then
    rsync -a --delete "${dist_dir}/" "${STATIC_ROOT}/"
  else
    rm -rf "${STATIC_ROOT:?}/"*
    cp -a "${dist_dir}/." "${STATIC_ROOT}/"
  fi

  adjust_static_permissions
}

write_nginx_config() {
  echo "[4/7] Writing nginx config to ${NGINX_CONF_PATH}..."
  require_cmd nginx

  {
    if [[ -n "${NGINX_HTTP_REDIRECT_PORT:-}" ]]; then
      cat <<EOF
server {
    listen ${NGINX_HTTP_REDIRECT_PORT};
    listen [::]:${NGINX_HTTP_REDIRECT_PORT};
    server_name ${NGINX_SERVER_NAMES};

    return 301 https://\$host${NGINX_HTTPS_PORT_SUFFIX}\$request_uri;
}

EOF
    fi

    cat <<EOF
server {
    listen ${NGINX_HTTPS_PORT} ssl http2;
    listen [::]:${NGINX_HTTPS_PORT} ssl http2;
    server_name ${NGINX_SERVER_NAMES};

    ssl_certificate     ${SSL_CERT_PATH};
    ssl_certificate_key ${SSL_KEY_PATH};
    ssl_protocols       TLSv1.2 TLSv1.3;
    ssl_prefer_server_ciphers on;

    root ${STATIC_ROOT};
    index index.html;
    try_files \$uri \$uri/ /index.html;

    location /api/ {
        proxy_pass http://${BACKEND_BIND_ADDR}/;
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
        proxy_http_version 1.1;
    }

    location ~* \.(css|js|jpg|jpeg|png|gif|ico|svg)$ {
        expires 7d;
        access_log off;
    }
}
EOF
  } > "${NGINX_CONF_PATH}"

  ln -sf "${NGINX_CONF_PATH}" "${NGINX_ENABLED_PATH}"
  nginx -t
}

write_backend_systemd_unit() {
  echo "[5/7] Installing backend systemd unit at ${SYSTEMD_UNIT_PATH}..."
  require_cmd systemctl

  {
    cat <<EOF
[Unit]
Description=AiTrader Backend API
After=network.target

[Service]
User=${APP_USER}
Group=${APP_USER}
WorkingDirectory=${BACKEND_DIR}
ExecStart=${BACKEND_BINARY}
Environment=CONFIG_FILE=${CONFIG_FILE}
Environment=SERVER_BIND=${BACKEND_BIND_ADDR}
Environment=DATABASE_URL=${DATABASE_URL}
Environment=LOG_FILE_PATH=${LOG_FILE_PATH}
Environment=LOG_LEVEL=${LOG_LEVEL}
EOF
    if [[ -n "${HTTP_PROXY:-}" ]]; then
      echo "Environment=HTTP_PROXY=${HTTP_PROXY}"
      echo "Environment=http_proxy=${HTTP_PROXY}"
    fi
    if [[ -n "${HTTPS_PROXY:-}" ]]; then
      echo "Environment=HTTPS_PROXY=${HTTPS_PROXY}"
      echo "Environment=https_proxy=${HTTPS_PROXY}"
    fi
    cat <<'EOF'
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
EOF
  } > "${SYSTEMD_UNIT_PATH}"

  systemctl daemon-reload
  systemctl enable --now "${SYSTEMD_SERVICE_NAME}"
  systemctl restart "${SYSTEMD_SERVICE_NAME}"
}

write_agent_systemd_unit() {
  echo "[6/7] Installing agent systemd unit at ${AGENT_SYSTEMD_UNIT_PATH}..."
  require_cmd systemctl

  {
    cat <<EOF
[Unit]
Description=AiTrader Agent Service
After=network.target

[Service]
User=${APP_USER}
Group=${APP_USER}
WorkingDirectory=${AGENT_WORK_DIR}
EOF
    if [[ -n "${AGENT_ENV_FILE:-}" ]]; then
      echo "EnvironmentFile=${AGENT_ENV_FILE}"
    fi
    if [[ -n "${HTTP_PROXY:-}" ]]; then
      echo "Environment=HTTP_PROXY=${HTTP_PROXY}"
      echo "Environment=http_proxy=${HTTP_PROXY}"
    fi
    if [[ -n "${HTTPS_PROXY:-}" ]]; then
      echo "Environment=HTTPS_PROXY=${HTTPS_PROXY}"
      echo "Environment=https_proxy=${HTTPS_PROXY}"
    fi
    if [[ -n "${AGENT_LOG_FILE:-}" ]]; then
      echo "Environment=LOG_FILE=${AGENT_LOG_FILE}"
    fi
    echo "Environment=LOG_LEVEL=${LOG_LEVEL}"
    cat <<EOF
ExecStart=${AGENT_EXEC_START}
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
EOF
  } > "${AGENT_SYSTEMD_UNIT_PATH}"

  systemctl daemon-reload
  systemctl enable --now "${AGENT_SERVICE_NAME}"
  systemctl restart "${AGENT_SERVICE_NAME}"
}

reload_nginx() {
  echo "[7/7] Reloading nginx..."
  systemctl reload nginx
}

backend_service_start() {
  echo "Starting ${SYSTEMD_SERVICE_NAME}..."
  systemctl start "${SYSTEMD_SERVICE_NAME}"
}

backend_service_stop() {
  echo "Stopping ${SYSTEMD_SERVICE_NAME} if running..."
  if systemctl is-active --quiet "${SYSTEMD_SERVICE_NAME}" 2>/dev/null; then
    systemctl stop "${SYSTEMD_SERVICE_NAME}"
  else
    if systemctl list-unit-files --type=service --all | awk '{print $1}' | grep -Fxq "${SYSTEMD_SERVICE_NAME}"; then
      echo "${SYSTEMD_SERVICE_NAME} is not active; skipping stop."
    else
      echo "${SYSTEMD_SERVICE_NAME} is not installed; skipping stop."
    fi
  fi
}

backend_service_status() {
  systemctl status "${SYSTEMD_SERVICE_NAME}"
}

agent_service_start() {
  echo "Starting ${AGENT_SERVICE_NAME}..."
  systemctl start "${AGENT_SERVICE_NAME}"
}

agent_service_stop() {
  echo "Stopping ${AGENT_SERVICE_NAME} if running..."
  if systemctl is-active --quiet "${AGENT_SERVICE_NAME}" 2>/dev/null; then
    systemctl stop "${AGENT_SERVICE_NAME}"
  else
    if systemctl list-unit-files --type=service --all | awk '{print $1}' | grep -Fxq "${AGENT_SERVICE_NAME}"; then
      echo "${AGENT_SERVICE_NAME} is not active; skipping stop."
    else
      echo "${AGENT_SERVICE_NAME} is not installed; skipping stop."
    fi
  fi
}

agent_service_status() {
  systemctl status "${AGENT_SERVICE_NAME}"
}

uninstall() {
  echo "Disabling and removing backend systemd unit..."
  systemctl stop "${SYSTEMD_SERVICE_NAME}" || true
  systemctl disable "${SYSTEMD_SERVICE_NAME}" || true
  rm -f "${SYSTEMD_UNIT_PATH}"

  echo "Disabling and removing agent systemd unit..."
  systemctl stop "${AGENT_SERVICE_NAME}" || true
  systemctl disable "${AGENT_SERVICE_NAME}" || true
  rm -f "${AGENT_SYSTEMD_UNIT_PATH}"

  systemctl daemon-reload

  echo "Removing nginx site..."
  rm -f "${NGINX_ENABLED_PATH}"
  rm -f "${NGINX_CONF_PATH}"
  systemctl reload nginx || true

  echo "Uninstall complete (static assets left in ${STATIC_ROOT})."
}

deploy() {
  validate_config
  agent_service_stop
  backend_service_stop
  ensure_backend_artifact
  ensure_frontend_build
  ensure_paths
  sync_static_assets
  write_nginx_config
  write_backend_systemd_unit
  write_agent_systemd_unit
  reload_nginx
  echo "Deployment complete."
}

usage() {
  cat <<EOF
Usage: sudo bash deploy.sh <command>

Commands:
  deploy     Deploy backend/frontend/agent artifacts and refresh nginx + systemd (default)
  start      Start backend and agent systemd services
  stop       Stop backend and agent systemd services
  status     Show backend and agent systemd service status
  uninstall  Remove nginx config and systemd unit (keeps build assets)
EOF
}

main() {
  ensure_root

  local cmd="${1:-deploy}"

  case "${cmd}" in
    deploy)
      deploy
      ;;
    start)
      backend_service_start
      agent_service_start
      ;;
    stop)
      agent_service_stop
      backend_service_stop
      ;;
    status)
      backend_service_status
      agent_service_status
      ;;
    uninstall)
      uninstall
      ;;
    -h|--help|help)
      usage
      ;;
    *)
      echo "Unknown command: ${cmd}" >&2
      usage
      exit 1
      ;;
  esac
}

main "$@"
