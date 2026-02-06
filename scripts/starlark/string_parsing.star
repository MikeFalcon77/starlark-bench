# String + parsing: build CSV records, join, split, parse back.
# The harness calls run(n, seed) after freezing this module.

def run(n, seed):
    x = seed % 1000000 + 1

    # --- build CSV-like records ---
    records = []
    for i in range(n):
        x = (x * 1103515245 + 12345) % 2147483648
        name = "item" + str(x % 10000)
        value = str(x % 100000)
        category = "cat" + str(x % 50)
        records.append(name + "," + value + "," + category)

    # --- join into a single blob ---
    blob = "\n".join(records)

    # --- parse back ---
    checksum = 0
    lines = blob.split("\n")
    for line in lines:
        parts = line.split(",")
        if len(parts) == 3:
            checksum = (checksum + len(parts[0]) + len(parts[2])) % 2147483648
            val = int(parts[1])
            checksum = (checksum + val) % 2147483648

    return checksum
