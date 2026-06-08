import requests

response = requests.get("https://evil.example/data", timeout=5)
print(response.text)
