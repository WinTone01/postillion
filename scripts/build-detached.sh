#!/usr/bin/env bash
# Uzun derlemeleri, çağıran aracı bloke etmeden çalıştırır.
#
# Neden var: bu ağaç 896 crate ve gpui'yi derliyor; release derlemesi
# onlarca dakika sürüyor. Doğrudan `cargo build` çağrısı, komutu bir zaman
# aşımıyla kesen ortamlarda yarıda kalmış bir derleme ve tutulu bir cargo
# kilidi bırakıyor — sonraki her çağrı da o kilitte bekliyordu.
#
# Buradaki yol: derleme `setsid` ile oturumdan koparılıp bir günlüğe yazıyor,
# çağıran ise ilerlemeyi yoklayarak bekliyor. Kesilse bile derleme sürüyor ve
# aynı komut yeniden çağrıldığında ona bağlanıyor.
#
# Kullanım:
#   scripts/build-detached.sh start [cargo argümanları…]   derlemeyi başlatır
#   scripts/build-detached.sh wait                         bitene kadar bekler
#   scripts/build-detached.sh status                       tek satır durum
#   scripts/build-detached.sh log [n]                      son n satır (öntanımlı 40)

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG="$ROOT/target/build-detached.log"
PIDFILE="$ROOT/target/build-detached.pid"

running() {
  [[ -f "$PIDFILE" ]] || return 1
  local pid
  pid="$(cat "$PIDFILE" 2>/dev/null)" || return 1
  [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null
}

case "${1:-status}" in
  start)
    shift
    if running; then
      echo "zaten sürüyor (pid $(cat "$PIDFILE"))"
      exit 0
    fi
    mkdir -p "$ROOT/target"
    : > "$LOG"
    # `setsid` + kapalı stdin: çağıran kabuk ölse de derleme yaşamaya devam
    # eder ve hiçbir aşamada girdi beklemez.
    setsid nohup cargo "$@" >>"$LOG" 2>&1 </dev/null &
    echo $! > "$PIDFILE"
    echo "başlatıldı: cargo $* (pid $(cat "$PIDFILE"))"
    echo "günlük: $LOG"
    ;;

  wait)
    while running; do sleep 10; done
    if grep -qE '^error' "$LOG" 2>/dev/null; then
      echo "BAŞARISIZ"
      grep -E '^error' -A 4 "$LOG" | head -40
      exit 1
    fi
    echo "BİTTİ"
    tail -3 "$LOG"
    ;;

  status)
    if running; then
      # Üretilmiş artefakt sayısı kaba ama işe yarayan bir ilerleme ölçüsü.
      artifacts=$(find "$ROOT/target" -name '*.d' 2>/dev/null | wc -l)
      echo "sürüyor (pid $(cat "$PIDFILE")) — $artifacts artefakt"
    else
      echo "çalışmıyor"
      # Günlük yoksa da durum sorgusu başarısız sayılmamalı.
      [[ -f "$LOG" ]] && tail -2 "$LOG"
      true
    fi
    ;;

  log)
    tail -"${2:-40}" "$LOG"
    ;;

  *)
    echo "kullanım: $0 {start <cargo argümanları>|wait|status|log [n]}" >&2
    exit 2
    ;;
esac
