out = "".join(chr(ord(c) ^ 42) for c in "payload")
print(out)