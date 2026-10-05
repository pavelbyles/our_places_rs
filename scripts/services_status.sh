#!/usr/bin/env bash
# scripts/services_status.sh — Inspect and report health and port status of Our Places development services
set -uo pipefail

# Default flags
WATCH_MODE=false
WATCH_INTERVAL=2
JSON_OUTPUT=false
STRICT_MODE=false
SHOW_ALL=false
NO_COLOR=false

# Help dialog
show_help() {
  cat <<'EOF'
Usage: services-status [OPTIONS]

Inspect and report the runtime health, ports, PIDs, and response status
of all development services (Database, Pub/Sub, APIs, and Frontends).

Options:
  -w, --watch [SEC]    Continuously monitor services (default: 2 seconds)
  -j, --json           Output service status as JSON
  -s, --strict         Exit with status 1 if any service is DOWN or DEGRADED
  -a, --all            Show all optional / additional services (e.g. image_worker)
      --no-color       Disable ANSI color escape sequences
  -h, --help           Display this help message and exit

Examples:
  services-status             # One-time health & port report
  services-status --watch     # Live auto-refreshing dashboard
  services-status --strict    # CI / script assertion (fails on degraded/down)
  services-status --json      # Programmatic inspection for tooling
EOF
}

# Parse command line options
while [[ $# -gt 0 ]]; do
  case "$1" in
    -w|--watch)
      WATCH_MODE=true
      if [[ $# -gt 1 ]] && [[ "$2" =~ ^[0-9]+$ ]]; then
        WATCH_INTERVAL="$2"
        shift 2
      else
        shift
      fi
      ;;
    -j|--json)
      JSON_OUTPUT=true
      shift
      ;;
    -s|--strict|--check)
      STRICT_MODE=true
      shift
      ;;
    -a|--all)
      SHOW_ALL=true
      shift
      ;;
    --no-color)
      NO_COLOR=true
      shift
      ;;
    -h|--help)
      show_help
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      echo "Run 'services-status --help' for usage." >&2
      exit 1
      ;;
  esac
done

# Configure colors
if [[ -t 1 ]] && [[ "$NO_COLOR" == false ]]; then
  C_RESET="\033[0m"
  C_BOLD="\033[1m"
  C_DIM="\033[2m"
  C_GREEN="\033[1;32m"
  C_RED="\033[1;31m"
  C_YELLOW="\033[1;33m"
  C_CYAN="\033[1;36m"
  C_BLUE="\033[1;34m"
else
  C_RESET=""
  C_BOLD=""
  C_DIM=""
  C_GREEN=""
  C_RED=""
  C_YELLOW=""
  C_CYAN=""
  C_BLUE=""
fi

# Multi-service PID cache
declare -A LISTEN_PIDS
declare -A LISTEN_CMDS

collect_pids() {
  LISTEN_PIDS=()
  LISTEN_CMDS=()
  if command -v lsof >/dev/null 2>&1; then
    while read -r cmd pid user fd type device size node name rest; do
      if [[ "$name" =~ :([0-9]+)$ ]]; then
        local port="${BASH_REMATCH[1]}"
        LISTEN_PIDS["$port"]="$pid"
        LISTEN_CMDS["$port"]="$cmd"
      fi
    done < <(lsof -iTCP:3000,3002,5432,8080,8081,8082,8083,8084,8085 -sTCP:LISTEN -n -P 2>/dev/null | tail -n +2 || true)
  fi
}

get_pid() {
  local port="$1"
  if [[ -n "${LISTEN_PIDS[$port]:-}" ]]; then
    echo "${LISTEN_PIDS[$port]}"
    return
  fi
  if command -v fuser >/dev/null 2>&1; then
    local fpid
    fpid=$(fuser "${port}/tcp" 2>/dev/null | awk '{print $1}' || true)
    if [[ -n "$fpid" ]]; then
      echo "$fpid"
      return
    fi
  fi
  if command -v ss >/dev/null 2>&1; then
    local spid
    spid=$(ss -tulpn "sport = :$port" 2>/dev/null | grep -o 'pid=[0-9]*' | head -n 1 | cut -d= -f2 || true)
    if [[ -n "$spid" ]]; then
      echo "$spid"
      return
    fi
  fi
  echo "-"
}

is_port_open() {
  local port="$1"
  (exec 3<>/dev/tcp/127.0.0.1/"$port") 2>/dev/null && exec 3<&- 3>&- && return 0
  return 1
}

run_check() {
  collect_pids

  # Define services: Category | Service Name | Port | Type (db, pubsub, api, frontend)
  local services=(
    "Database|PostgreSQL|5432|db"
    "Pub/Sub|Pub/Sub Emulator|8085|pubsub"
    "API|Email Worker|8080|api"
    "API|Booking API|8081|api"
    "API|Listing API|8082|api"
    "API|User API|8083|api"
    "Frontend|Guest Portal (web_app_tc)|3000|frontend"
    "Frontend|Admin Portal (web_app_admin_tc)|3002|frontend"
  )

  # Include Image Worker if active or --all requested
  if [[ "$SHOW_ALL" == true ]] || is_port_open 8084; then
    services+=("API|Image Worker|8084|api")
  fi

  local count_total=${#services[@]}
  local count_up=0
  local count_down=0
  local count_degraded=0

  # Data collection arrays
  local res_cats=()
  local res_names=()
  local res_ports=()
  local res_statuses=()
  local res_pids=()
  local res_details=()

  for entry in "${services[@]}"; do
    IFS="|" read -r cat name port svc_type <<< "$entry"
    local status="DOWN"
    local pid="-"
    local details=""

    if ! is_port_open "$port"; then
      status="DOWN"
      details="Port closed / not running"
      count_down=$((count_down + 1))
    else
      pid=$(get_pid "$port")

      case "$svc_type" in
        db)
          if pg_isready -h 127.0.0.1 -p "$port" -q 2>/dev/null; then
            status="UP"
            details="Accepting connections"
            count_up=$((count_up + 1))
          else
            status="DEGRADED"
            details="Port open, query check failed"
            count_degraded=$((count_degraded + 1))
          fi
          ;;

        pubsub)
          local code
          code=$(curl -s -o /dev/null -w "%{http_code}" --connect-timeout 1 --max-time 2 "http://127.0.0.1:${port}/" 2>/dev/null || echo "000")
          if [[ "$code" == "200" ]]; then
            status="UP"
            local topics
            topics=$(curl -s --connect-timeout 1 --max-time 2 "http://127.0.0.1:${port}/v1/projects/our-places-dev/topics" 2>/dev/null | grep -o 'projects/our-places-dev/topics/[^"]*' | wc -l || echo "0")
            if [[ "$topics" -gt 0 ]]; then
              details="Emulator active ($topics topic$([[ $topics -ne 1 ]] && echo "s"))"
            else
              details="Emulator active (HTTP 200 OK)"
            fi
            count_up=$((count_up + 1))
          else
            status="DEGRADED"
            details="HTTP $code"
            count_degraded=$((count_degraded + 1))
          fi
          ;;

        api)
          local resp code
          resp=$(curl -s --connect-timeout 1 --max-time 2 "http://127.0.0.1:${port}/health" 2>/dev/null || echo "")
          code=$(curl -s -o /dev/null -w "%{http_code}" --connect-timeout 1 --max-time 2 "http://127.0.0.1:${port}/health" 2>/dev/null || echo "000")
          if [[ "$code" == "200" ]]; then
            status="UP"
            local db_status db_lat
            db_status=$(echo "$resp" | jq -r '.components.db.status // empty' 2>/dev/null || echo "")
            db_lat=$(echo "$resp" | jq -r '.components.db.details.latency_ms // empty' 2>/dev/null || echo "")
            if [[ -n "$db_lat" ]]; then
              details="Healthy (HTTP 200, DB: UP ${db_lat}ms)"
            elif [[ "$db_status" == "UP" ]]; then
              details="Healthy (HTTP 200, DB: UP)"
            else
              details="Healthy (HTTP 200 OK)"
            fi
            count_up=$((count_up + 1))
          elif [[ "$code" == "503" ]]; then
            status="DEGRADED"
            details="Degraded (HTTP 503)"
            count_degraded=$((count_degraded + 1))
          else
            status="DEGRADED"
            details="HTTP $code"
            count_degraded=$((count_degraded + 1))
          fi
          ;;

        frontend)
          local code
          code=$(curl -s -o /dev/null -w "%{http_code}" --connect-timeout 1 --max-time 2 "http://127.0.0.1:${port}/" 2>/dev/null || echo "000")
          if [[ "$code" == "200" || "$code" == "302" || "$code" == "301" ]]; then
            status="UP"
            details="Serving (HTTP $code OK)"
            count_up=$((count_up + 1))
          else
            status="DEGRADED"
            details="HTTP $code"
            count_degraded=$((count_degraded + 1))
          fi
          ;;
      esac
    fi

    res_cats+=("$cat")
    res_names+=("$name")
    res_ports+=("$port")
    res_statuses+=("$status")
    res_pids+=("$pid")
    res_details+=("$details")
  done

  # JSON Mode
  if [[ "$JSON_OUTPUT" == true ]]; then
    local ts
    ts=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
    printf '{\n  "timestamp": "%s",\n  "summary": {\n    "total": %d,\n    "up": %d,\n    "down": %d,\n    "degraded": %d\n  },\n  "services": [\n' \
      "$ts" "$count_total" "$count_up" "$count_down" "$count_degraded"
    for ((i=0; i<count_total; i++)); do
      printf '    {\n      "category": "%s",\n      "name": "%s",\n      "port": %s,\n      "status": "%s",\n      "pid": %s,\n      "details": "%s"\n    }%s\n' \
        "${res_cats[i]}" "${res_names[i]}" "${res_ports[i]}" "${res_statuses[i]}" \
        "$([[ "${res_pids[i]}" =~ ^[0-9]+$ ]] && echo "${res_pids[i]}" || echo "null")" \
        "${res_details[i]}" \
        "$([[ $i -lt $((count_total - 1)) ]] && echo "," || echo "")"
    done
    printf '  ]\n}\n'

    if [[ "$STRICT_MODE" == true ]] && [[ $count_down -gt 0 || $count_degraded -gt 0 ]]; then
      return 1
    fi
    return 0
  fi

  # Terminal Table Mode
  printf "${C_BOLD}%s${C_RESET}\n" "Our Places — Development Services Status"
  printf "${C_DIM}%s${C_RESET}\n\n" "Inspected at: $(date '+%Y-%m-%d %H:%M:%S %Z')"

  printf "┌────────────┬──────────────────────────────────┬────────┬──────────┬─────────┬──────────────────────────────────────────┐\n"
  printf "│ %-10s │ %-32s │ %-6s │ %-8s │ %-7s │ %-40s │\n" "Category" "Service" "Port" "Status" "PID" "Details"
  printf "├────────────┼──────────────────────────────────┼────────┼──────────┼─────────┼──────────────────────────────────────────┤\n"

  for ((i=0; i<count_total; i++)); do
    local cat="${res_cats[i]}"
    local name="${res_names[i]}"
    local port="${res_ports[i]}"
    local status="${res_statuses[i]}"
    local pid="${res_pids[i]}"
    local details="${res_details[i]}"

    local color_prefix="$C_RESET"
    case "$status" in
      UP)       color_prefix="$C_GREEN" ;;
      DOWN)     color_prefix="$C_RED" ;;
      DEGRADED) color_prefix="$C_YELLOW" ;;
    esac

    printf "│ %-10s │ %-32s │ %-6s │ ${color_prefix}%-8s${C_RESET} │ %-7s │ %-40s │\n" \
      "$cat" "$name" "$port" "$status" "$pid" "$details"
  done

  printf "└────────────┴──────────────────────────────────┴────────┴──────────┴─────────┴──────────────────────────────────────────┘\n"

  # Summary line
  if [[ $count_down -eq 0 && $count_degraded -eq 0 ]]; then
    printf "${C_GREEN}${C_BOLD}✓ Operational:${C_RESET} All %d services are running and healthy.\n" "$count_total"
  else
    printf "${C_YELLOW}${C_BOLD}⚠️  Warning:${C_RESET} %d/%d services UP" "$count_up" "$count_total"
    if [[ $count_down -gt 0 ]]; then
      printf ", ${C_RED}%d DOWN${C_RESET}" "$count_down"
    fi
    if [[ $count_degraded -gt 0 ]]; then
      printf ", ${C_YELLOW}%d DEGRADED${C_RESET}" "$count_degraded"
    fi
    printf "\n\n"

    printf "${C_BOLD}Service Recovery Helpers:${C_RESET}\n"
    printf "  • Start everything (APIs + Frontends + DB): ${C_CYAN}fullstack${C_RESET}\n"
    printf "  • Start APIs + DB in background:           ${C_CYAN}apis-start${C_RESET} (or ${C_CYAN}apis${C_RESET} for foreground)\n"
    printf "  • Start Topcoat frontends:                 ${C_CYAN}frontends${C_RESET}\n"
    printf "  • Start Pub/Sub emulator:                  ${C_CYAN}pubsub-start${C_RESET}\n"
    printf "  • Start PostgreSQL database:               ${C_CYAN}db-start${C_RESET}\n"
  fi

  if [[ "$STRICT_MODE" == true ]] && [[ $count_down -gt 0 || $count_degraded -gt 0 ]]; then
    return 1
  fi
  return 0
}

# Execution loop
if [[ "$WATCH_MODE" == false ]]; then
  run_check
  exit $?
fi

# Watch mode (loop until Ctrl+C)
trap 'echo -e "\nMonitoring stopped."; exit 0' SIGINT SIGTERM

while true; do
  # Clear screen for interactive dashboard
  if [[ "$NO_COLOR" == false ]]; then
    printf "\033[H\033[2J"
  else
    clear 2>/dev/null || true
  fi
  run_check || true
  printf "\n${C_DIM}Refreshing every %ds. Press Ctrl+C to exit.${C_RESET}\n" "$WATCH_INTERVAL"
  sleep "$WATCH_INTERVAL"
done
