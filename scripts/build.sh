#!/bin/bash

which idf.py >/dev/null || {
    source ~/export-esp.sh >/dev/null 2>&1
}

case "$1" in
"" | "release")
    cargo build --release --features=device_nvs
    ;;
"debug")
    cargo build --features=device_nvs

    ;;
*)
    echo "Wrong argument. Only \"debug\"/\"release\" arguments are supported"
    exit 1
    ;;
esac
