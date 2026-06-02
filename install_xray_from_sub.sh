#!/usr/bin/env bash
set -euo pipefail

# ====== 只改这里 ======
SUB_URL='https://316.sub987.top/weibo/ipx/client/dy?token=b88ee1cdd27afde5f25c55a8f32c63c7'
CHECK_URL='https://www.gstatic.com/generate_204'
MAX_BENCH_NODES=40
BENCH_INTERVAL_SECONDS=1800
PROBE_TIMEOUT_SECONDS=10
# =====================

WORK_DIR='/tmp/xray_sub_build'
RAW_TXT="$WORK_DIR/sub_raw.txt"
SUB_TXT="$WORK_DIR/sub.txt"
VLESS_TXT="$WORK_DIR/vless.txt"

XRAY_ETC='/usr/local/etc/xray'
NODES_FILE="$XRAY_ETC/nodes.txt"
MANAGER_ENV="$XRAY_ETC/manager.env"
MANAGER_BIN='/usr/local/bin/xray_node_manager.sh'
MANAGER_SERVICE='/etc/systemd/system/xray-node-manager.service'
MANAGER_TIMER='/etc/systemd/system/xray-node-manager.timer'

mkdir -p "$WORK_DIR"

echo '[1/7] 检查依赖...'
for bin in curl python3 systemctl grep awk base64; do
  command -v "$bin" >/dev/null || { echo "缺少依赖: $bin"; exit 1; }
done

echo '[2/7] 安装 xray（如已安装会跳过）...'
if ! command -v xray >/dev/null; then
  curl -Ls https://github.com/XTLS/Xray-install/raw/main/install-release.sh | bash -s -- install
else
  echo 'xray 已安装，跳过安装。'
fi

echo '[3/7] 拉取并解析订阅...'
curl -fsSL "$SUB_URL" -o "$RAW_TXT"

if grep -qE '^vless://' "$RAW_TXT"; then
  cp "$RAW_TXT" "$SUB_TXT"
else
  if ! tr -d '\r\n' < "$RAW_TXT" | base64 -d > "$SUB_TXT" 2>/dev/null; then
    echo '订阅既不是明文 vless 列表，也不是可解码的 base64。'
    exit 1
  fi
fi

tr -d '\r' < "$SUB_TXT" | grep -E '^vless://' > "$VLESS_TXT" || true
if [[ ! -s "$VLESS_TXT" ]]; then
  echo '订阅中没有 vless:// 节点，请检查链接有效性。'
  exit 1
fi

echo '[4/7] 写入节点池与参数...'
sudo install -d -m 755 "$XRAY_ETC"
sudo install -m 600 "$VLESS_TXT" "$NODES_FILE"

sudo tee "$MANAGER_ENV" >/dev/null <<EOF
CHECK_URL='$CHECK_URL'
MAX_BENCH_NODES=$MAX_BENCH_NODES
BENCH_INTERVAL_SECONDS=$BENCH_INTERVAL_SECONDS
PROBE_TIMEOUT_SECONDS=$PROBE_TIMEOUT_SECONDS
NODES_FILE='$NODES_FILE'
CONFIG_PATH='$XRAY_ETC/config.json'
CURRENT_FILE='$XRAY_ETC/current_node.url'
LAST_BENCH_FILE='$XRAY_ETC/last_benchmark_epoch'
SOCKS_ADDR='127.0.0.1:10808'
EOF
sudo chmod 600 "$MANAGER_ENV"

echo '[5/7] 安装节点管理器（自动切换+自动测速）...'
sudo tee "$MANAGER_BIN" >/dev/null <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

if [[ -f /usr/local/etc/xray/manager.env ]]; then
  # shellcheck disable=SC1091
  source /usr/local/etc/xray/manager.env
fi

CHECK_URL="${CHECK_URL:-https://www.gstatic.com/generate_204}"
MAX_BENCH_NODES="${MAX_BENCH_NODES:-40}"
BENCH_INTERVAL_SECONDS="${BENCH_INTERVAL_SECONDS:-1800}"
PROBE_TIMEOUT_SECONDS="${PROBE_TIMEOUT_SECONDS:-10}"
NODES_FILE="${NODES_FILE:-/usr/local/etc/xray/nodes.txt}"
CONFIG_PATH="${CONFIG_PATH:-/usr/local/etc/xray/config.json}"
CURRENT_FILE="${CURRENT_FILE:-/usr/local/etc/xray/current_node.url}"
LAST_BENCH_FILE="${LAST_BENCH_FILE:-/usr/local/etc/xray/last_benchmark_epoch}"
SOCKS_ADDR="${SOCKS_ADDR:-127.0.0.1:10808}"
NODE_REGION="${NODE_REGION:-US}"

log() {
  echo "[xray-node-manager] $*"
}

need_root() {
  if [[ "${EUID}" -ne 0 ]]; then
    log '请以 root 运行（systemd 已默认 root）。'
    exit 1
  fi
}

float_lt() {
  awk -v a="$1" -v b="$2" 'BEGIN{exit (a+0 < b+0) ? 0 : 1}'
}

node_matches_region() {
  local node_url="$1"
  NODE_URL="$node_url" NODE_REGION="$NODE_REGION" python3 - <<'PY'
import os
import re
import sys
from urllib.parse import unquote, urlparse

region = os.environ.get('NODE_REGION', 'US').strip().upper()
u = os.environ['NODE_URL'].strip()

p = urlparse(u)
label = unquote(p.fragment or '')
host = (p.hostname or '').lower()
haystack = f'{label} {host}'.lower()

if region == 'US':
    patterns = [
        r'(^|[^a-z0-9])us([^a-z0-9]|$)',
        r'(^|[^a-z0-9])us[0-9]',
        r'\[us\]',
        r'united states',
        r'america',
        r'美国',
        r'los angeles',
        r'santa clara',
        r'san jose',
        r'san francisco',
    ]
    sys.exit(0 if any(re.search(pattern, haystack) for pattern in patterns) else 1)

sys.exit(0 if region.lower() in haystack else 1)
PY
}

load_region_nodes() {
  local node
  while IFS= read -r node; do
    [[ -n "$node" ]] || continue
    if node_matches_region "$node"; then
      printf '%s\n' "$node"
    fi
  done < "$NODES_FILE"
}

generate_config() {
  local node_url="$1"
  local out_path="$2"
  NODE_URL="$node_url" OUTPUT_PATH="$out_path" python3 - <<'PY'
import json
import os
import sys
from urllib.parse import parse_qs, urlparse

u = os.environ['NODE_URL'].strip()
o = os.environ['OUTPUT_PATH']

p = urlparse(u)
q = parse_qs(p.query)

uuid = p.username or ''
address = p.hostname or ''
port = p.port or 443
flow = q.get('flow', ['xtls-rprx-vision'])[0]
fp = q.get('fp', ['chrome'])[0]
sni = q.get('sni', [''])[0]
pbk = q.get('pbk', [''])[0]
sid = q.get('sid', [''])[0]

missing = [k for k, v in [('uuid', uuid), ('address', address), ('sni', sni), ('pbk', pbk)] if not v]
if missing:
    print('missing fields:', ','.join(missing), file=sys.stderr)
    sys.exit(2)

cfg = {
    'log': {'loglevel': 'warning'},
    'inbounds': [
        {
            'tag': 'socks-in',
            'listen': '127.0.0.1',
            'port': 10808,
            'protocol': 'socks',
            'settings': {'auth': 'noauth', 'udp': True},
        },
        {
            'tag': 'http-in',
            'listen': '127.0.0.1',
            'port': 10809,
            'protocol': 'http',
            'settings': {},
        },
    ],
    'outbounds': [
        {
            'tag': 'proxy',
            'protocol': 'vless',
            'settings': {
                'vnext': [
                    {
                        'address': address,
                        'port': port,
                        'users': [{'id': uuid, 'encryption': 'none', 'flow': flow}],
                    }
                ]
            },
            'streamSettings': {
                'network': 'tcp',
                'security': 'reality',
                'realitySettings': {
                    'serverName': sni,
                    'fingerprint': fp,
                    'publicKey': pbk,
                    'shortId': sid,
                },
            },
        },
        {'tag': 'direct', 'protocol': 'freedom'},
        {'tag': 'block', 'protocol': 'blackhole'},
    ],
}

with open(o, 'w', encoding='utf-8') as f:
    json.dump(cfg, f, ensure_ascii=False, indent=2)
PY
}

apply_config() {
  local tmp_cfg="$1"
  install -m 644 -o nobody -g nogroup "$tmp_cfg" "$CONFIG_PATH"
  systemctl restart xray
  sleep 1
}

probe_latency() {
  local t
  t="$(curl --socks5-hostname "$SOCKS_ADDR" -o /dev/null -sS -w '%{time_total}' --max-time "$PROBE_TIMEOUT_SECONDS" "$CHECK_URL" 2>/dev/null || true)"
  [[ -n "$t" ]] || return 1
  awk -v x="$t" 'BEGIN{exit (x+0 > 0) ? 0 : 1}' || return 1
  echo "$t"
}

switch_to_node() {
  local node_url="$1"
  local tmp_cfg
  tmp_cfg="$(mktemp)"

  if ! generate_config "$node_url" "$tmp_cfg"; then
    rm -f "$tmp_cfg"
    return 1
  fi
  if ! apply_config "$tmp_cfg"; then
    rm -f "$tmp_cfg"
    return 1
  fi
  rm -f "$tmp_cfg"

  if latency="$(probe_latency)"; then
    printf '%s\n' "$node_url" > "$CURRENT_FILE"
    log "切换成功，延迟: ${latency}s"
    return 0
  fi
  return 1
}

failover_rotate() {
  [[ -s "$NODES_FILE" ]] || { log '节点池为空'; return 1; }

  mapfile -t nodes < <(load_region_nodes)
  local total="${#nodes[@]}"
  (( total > 0 )) || { log "没有匹配 ${NODE_REGION} 的节点"; return 1; }

  local current='' start=0 i idx
  if [[ -f "$CURRENT_FILE" ]]; then
    current="$(cat "$CURRENT_FILE" 2>/dev/null || true)"
  fi

  if [[ -n "$current" ]]; then
    for i in "${!nodes[@]}"; do
      if [[ "${nodes[$i]}" == "$current" ]]; then
        start=$(( (i + 1) % total ))
        break
      fi
    done
  fi

  for ((i=0; i<total; i++)); do
    idx=$(( (start + i) % total ))
    [[ -n "${nodes[$idx]}" ]] || continue
    log "故障切换尝试 ${NODE_REGION} 节点 $((idx+1))/$total"
    if switch_to_node "${nodes[$idx]}"; then
      return 0
    fi
  done
  return 1
}

benchmark_fastest() {
  [[ -s "$NODES_FILE" ]] || { log '节点池为空'; return 1; }

  local best_node=''
  local best_latency=''
  local count=0
  local node latency tmp_cfg

  log "只在 ${NODE_REGION} 节点中测速择优"
  while IFS= read -r node; do
    [[ -n "$node" ]] || continue
    count=$((count + 1))
    if (( count > MAX_BENCH_NODES )); then
      break
    fi

    tmp_cfg="$(mktemp)"
    if ! generate_config "$node" "$tmp_cfg"; then
      rm -f "$tmp_cfg"
      continue
    fi
    if ! apply_config "$tmp_cfg"; then
      rm -f "$tmp_cfg"
      continue
    fi
    rm -f "$tmp_cfg"

    if latency="$(probe_latency)"; then
      log "测速 $count: ${latency}s"
      if [[ -z "$best_latency" ]] || float_lt "$latency" "$best_latency"; then
        best_latency="$latency"
        best_node="$node"
      fi
    else
      log "测速 $count: 失败"
    fi
  done < <(load_region_nodes)

  if (( count == 0 )); then
    log "没有匹配 ${NODE_REGION} 的节点"
    return 1
  fi
  [[ -n "$best_node" ]] || return 1

  if [[ "$(cat "$CURRENT_FILE" 2>/dev/null || true)" != "$best_node" ]]; then
    log '应用最快节点...'
    switch_to_node "$best_node" >/dev/null
  fi

  date +%s > "$LAST_BENCH_FILE"
  log "最快节点延迟: ${best_latency}s"
  return 0
}

need_benchmark_now() {
  local now last
  now="$(date +%s)"
  if [[ ! -f "$LAST_BENCH_FILE" ]]; then
    return 0
  fi
  last="$(cat "$LAST_BENCH_FILE" 2>/dev/null || echo 0)"
  (( now - last >= BENCH_INTERVAL_SECONDS ))
}

ensure_current_alive() {
  local current=''
  if [[ -f "$CURRENT_FILE" ]]; then
    current="$(cat "$CURRENT_FILE" 2>/dev/null || true)"
  fi

  if [[ -n "$current" ]] && ! node_matches_region "$current"; then
    log "当前节点不属于 ${NODE_REGION}，开始自动切换..."
    failover_rotate
    return
  fi

  if [[ -n "$current" ]] && probe_latency >/dev/null 2>&1; then
    return 0
  fi
  log '当前节点不可用，开始自动切换...'
  failover_rotate
}

auto_run() {
  ensure_current_alive || log '故障切换失败：没有可用节点。'
  if need_benchmark_now; then
    log '开始周期测速并切换最快节点...'
    benchmark_fastest || log '测速失败：未找到可用节点。'
  fi
}

main() {
  need_root
  local mode="${1:-auto}"
  case "$mode" in
    auto)
      auto_run
      ;;
    failover)
      ensure_current_alive
      ;;
    benchmark)
      benchmark_fastest
      ;;
    *)
      log '用法: xray_node_manager.sh [auto|failover|benchmark]'
      exit 2
      ;;
  esac
}

main "$@"
EOF
sudo chmod 755 "$MANAGER_BIN"

echo '[6/7] 配置 systemd 定时自动管理...'
sudo tee "$MANAGER_SERVICE" >/dev/null <<'EOF'
[Unit]
Description=Xray Node Auto Manager
After=network-online.target xray.service
Wants=network-online.target

[Service]
Type=oneshot
ExecStart=/usr/local/bin/xray_node_manager.sh auto
EOF

sudo tee "$MANAGER_TIMER" >/dev/null <<'EOF'
[Unit]
Description=Run Xray Node Auto Manager periodically

[Timer]
OnBootSec=2min
OnUnitActiveSec=5min
Persistent=true

[Install]
WantedBy=timers.target
EOF

echo '[7/7] 首次测速选最快节点并启动定时器...'
sudo systemctl daemon-reload
sudo systemctl enable xray >/dev/null 2>&1 || true
sudo "$MANAGER_BIN" benchmark
sudo systemctl enable --now xray-node-manager.timer

echo
echo '当前 xray 服务状态：'
sudo systemctl --no-pager --full status xray | sed -n '1,12p'
echo
echo '当前定时器状态：'
sudo systemctl --no-pager --full status xray-node-manager.timer | sed -n '1,12p'
echo
echo 'SOCKS5 代理测试：'
curl --socks5-hostname 127.0.0.1:10808 https://api.ip.sb/ip --max-time 20 || true
echo
echo '完成：已启用“故障自动切换 + 周期测速择优切换”。'
echo '手动触发故障切换: sudo /usr/local/bin/xray_node_manager.sh failover'
echo '手动触发全量测速: sudo /usr/local/bin/xray_node_manager.sh benchmark'
