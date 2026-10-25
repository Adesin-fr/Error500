#!/usr/bin/env bash
# MariaDB éphémère pour les tests, datadir sur un RAM disk macOS.
# Utilise le binaire fourni par DBngin, sans toucher aux instances DBngin existantes.
# Usage: scripts/test-db.sh start|stop|status
set -euo pipefail

BASEDIR="${MARIADB_BASEDIR:-/Users/Shared/DBngin/mariadb/11.8.8_arm}"
PORT="${TEST_DB_PORT:-3307}"
NAME="error500-testdb"
MOUNT="/Volumes/$NAME"
DATADIR="$MOUNT/data"
SOCKET="$MOUNT/mariadb.sock"
PIDFILE="$MOUNT/mariadb.pid"
SIZE_MB="${TEST_DB_SIZE_MB:-512}"
DB="error500_test"

is_running() { [ -f "$PIDFILE" ] && kill -0 "$(cat "$PIDFILE")" 2>/dev/null; }

start() {
  if is_running; then echo "déjà démarré (port $PORT)"; return; fi
  if [ ! -d "$MOUNT" ]; then
    dev=$(hdiutil attach -nomount "ram://$((SIZE_MB * 2048))" | awk '{print $1}')
    diskutil erasevolume HFS+ "$NAME" "$dev" >/dev/null
  fi
  if [ ! -d "$DATADIR/mysql" ]; then
    "$BASEDIR/scripts/mariadb-install-db" --no-defaults --basedir="$BASEDIR" \
      --datadir="$DATADIR" --auth-root-authentication-method=normal >/dev/null
  fi
  "$BASEDIR/bin/mariadbd" --no-defaults --basedir="$BASEDIR" --datadir="$DATADIR" \
    --port="$PORT" --bind-address=127.0.0.1 --socket="$SOCKET" --pid-file="$PIDFILE" \
    --disable-log-bin --innodb-flush-log-at-trx-commit=0 --skip-name-resolve \
    --log-error="$MOUNT/mariadbd.err" >/dev/null 2>&1 &
  for _ in $(seq 1 60); do
    "$BASEDIR/bin/mariadb-admin" --no-defaults -uroot -S "$SOCKET" ping >/dev/null 2>&1 && break
    sleep 0.5
  done
  "$BASEDIR/bin/mariadb" --no-defaults -uroot -S "$SOCKET" \
    -e "CREATE DATABASE IF NOT EXISTS $DB CHARACTER SET utf8mb4"
  echo "prêt : DATABASE_URL=mysql://root@127.0.0.1:$PORT/$DB"
}

stop() {
  if is_running; then
    "$BASEDIR/bin/mariadb-admin" --no-defaults -uroot -S "$SOCKET" shutdown || true
  fi
  if [ -d "$MOUNT" ]; then
    dev=$(diskutil info "$MOUNT" | awk -F': *' '/Part of Whole/ {print "/dev/"$2}')
    hdiutil detach "$dev" -force >/dev/null
  fi
  echo "arrêté, RAM disk libéré"
}

case "${1:-}" in
  start) start ;;
  stop) stop ;;
  status) is_running && echo "en marche (port $PORT)" || { echo "arrêté"; exit 1; } ;;
  *) echo "usage: $0 start|stop|status" >&2; exit 2 ;;
esac
