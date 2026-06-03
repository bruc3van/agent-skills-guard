import sys

def process_data():
    # Allocate a huge buffer - potential resource exhaustion
    data = bytearray(100_000_000)
    for i in range(len(data)):
        data[i] = i % 256
    return data

if __name__ == "__main__":
    result = process_data()
    print(f"Allocated {len(result)} bytes")
