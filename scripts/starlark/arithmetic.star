# Arithmetic loop: integer ops, float ops, mixed ops, branching.
# The harness calls run(n, seed) after freezing this module.

def run(n, seed):
    x = seed % 1000000 + 1
    y = (seed + 7) % 1000000 + 1
    z = 0.5
    checksum = 0

    for i in range(n):
        # LCG-style integer updates
        x = (x * 1103515245 + 12345) % 2147483648
        y = (y * 214013 + 2531011) % 2147483648

        # Float accumulation
        z = z + float(x % 1000) / 1000.0 - 0.5
        if z > 1000.0:
            z = z - 1000.0
        if z < -1000.0:
            z = z + 1000.0

        # Branching on remainder
        r = x % 5
        if r == 0:
            checksum = (checksum + x) % 2147483648
        elif r == 1:
            checksum = (checksum + y) % 2147483648
        elif r == 2:
            checksum = (checksum + int(z * 100)) % 2147483648
        elif r == 3:
            checksum = (checksum + (x % 1000) * (y % 1000)) % 2147483648
        else:
            checksum = (checksum + (x + y) % 1000000) % 2147483648

    return checksum
