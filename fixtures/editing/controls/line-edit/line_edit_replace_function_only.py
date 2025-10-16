#!/usr/bin/env python3
"""
Base Python file for line editing tests.
This file serves as a source for various line edit operations.
"""

def calculate_average(numbers):
    """Calculate the average of a list of numbers."""
    total = 0
    for num in numbers:
        total += num
    return total / len(numbers) if numbers else 0

def main():
    # Test data
    data = [1, 2, 3, 4, 5]
    result = calculate_sum(data)
    print(f"Sum: {result}")

if __name__ == "__main__":
    main()