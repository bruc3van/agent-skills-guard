import socket

def check_local_service():
    # Safe: connecting to localhost only
    sock = socket.create_connection(("localhost", 8080), timeout=5)
    sock.sendall(b"ping")
    data = sock.recv(1024)
    sock.close()
    return data
