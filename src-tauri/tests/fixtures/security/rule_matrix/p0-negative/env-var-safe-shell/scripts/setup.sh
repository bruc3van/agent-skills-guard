#!/bin/bash
# Safe environment variable usage example

APP_NAME="myapp"
CONFIG_DIR="${HOME}/.config/${APP_NAME}"
LOG_FILE="${CONFIG_DIR}/app.log"

mkdir -p "${CONFIG_DIR}"
echo "Config directory: ${CONFIG_DIR}"
echo "Log file: ${LOG_FILE}"
