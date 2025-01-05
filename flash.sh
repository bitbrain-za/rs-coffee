#!/bin/bash

source home.env

echo "Using"
echo "WIFI SSID: $WIFI_SSID"
echo "WIFI PASSWORD: $WIFI_PASSWORD"

./scripts/flash.sh 