import os

user = "attacker"
path = os.path.join("/tmp", user)
open(path)