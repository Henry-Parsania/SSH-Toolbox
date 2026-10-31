#!/bin/sh
# Q2: memory used by SSH Toolbox with one terminal open, including its webview processes.
# Start the app, wait until the prompt shows, then run:  sh scripts/measure-memory.sh [name]
# Uses PSS (proportional set size), so memory shared between processes isn't counted twice.
name="${1:-app}"

root=$(pgrep -x "$name" | head -n 1)
if [ -z "$root" ]; then
    echo "No running process called '$name'. Start the app first." >&2
    exit 1
fi

# The app and everything below it.
pids="$root"
frontier="$root"
while [ -n "$frontier" ]; do
    next=""
    for p in $frontier; do
        kids=$(pgrep -P "$p")
        [ -n "$kids" ] && next="$next $kids"
    done
    pids="$pids $next"
    frontier="$next"
done

total=0
count=0
for p in $pids; do
    f="/proc/$p/smaps_rollup"
    [ -r "$f" ] || continue
    kb=$(awk '/^Pss:/ {print $2}' "$f")
    [ -n "$kb" ] || continue
    total=$((total + kb))
    count=$((count + 1))
    printf '%8s MB  %s (pid %s)\n' "$((kb / 1024))" "$(cat /proc/$p/comm)" "$p"
done
echo "-------------------------------------"
printf '%8s MB  total for %s processes (target: under 250 MB)\n' "$((total / 1024))" "$count"
