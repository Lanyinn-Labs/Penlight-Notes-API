#!/bin/sh
set -eu

# These tools operate on an explicitly supplied file and need no remote refresh.
case "${1:-}" in
    check-client-config|master-import) exec penlight-notes-api "$@" ;;
esac

config=${PENLIGHT_CLIENT_CONFIG:-/app/runtime/jp-client.json}
export PENLIGHT_CLIENT_CONFIG="$config"
mkdir -p "$(dirname "$config")"
if [ ! -f "$config" ] || ! penlight-notes-api check-client-config "$config" >/dev/null; then
    # A newer image may no longer contain the cached configuration's protocol.
    # Start from this image's validated baseline before attempting a refresh.
    penlight-notes-api check-client-config /app/data/jp-client.json >/dev/null
    candidate=$(mktemp "${config}.XXXXXX")
    trap 'rm -f "$candidate"' EXIT HUP INT TERM
    cp /app/data/jp-client.json "$candidate"
    mv "$candidate" "$config"
    trap - EXIT HUP INT TERM
fi

if [ -n "${PENLIGHT_CLIENT_CONFIG_URL:-}" ]; then
    candidate=$(mktemp "${config}.XXXXXX")
    trap 'rm -f "$candidate"' EXIT HUP INT TERM
    if curl --fail --silent --show-error --proto '=https' \
        --connect-timeout 3 --max-time 10 --max-filesize 65536 \
        --output "$candidate" "$PENLIGHT_CLIENT_CONFIG_URL" \
        && penlight-notes-api check-client-config "$candidate" >/dev/null; then
        mv "$candidate" "$config"
        echo 'Updated runtime client configuration' >&2
    else
        echo 'Client configuration refresh failed; keeping the last valid configuration' >&2
    fi
    rm -f "$candidate"
    trap - EXIT HUP INT TERM
fi

exec penlight-notes-api "$@"
