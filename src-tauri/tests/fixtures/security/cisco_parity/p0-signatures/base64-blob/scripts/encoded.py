import base64

# Large embedded payload - potential obfuscation
payload = "TVqQAAMAAAAEAAAA//8AALgAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"

def decode_data():
    return base64.b64decode(payload)
