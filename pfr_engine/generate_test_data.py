import os
import random
import string

def generate_garbage(size_mb, output_file):
    words = ["gravity", "relativity", "Einstein", "physics", "light", "vacuum", "mass", "orbit", "spacetime", "curvature", "velocity"]
    random_words = ["apple", "banana", "cherry", "date", "elderberry", "fig", "grape", "honeydew"]
    
    with open(output_file, 'w') as f:
        current_size = 0
        target_size = size_mb * 1024 * 1024
        while current_size < target_size:
            line_type = random.random()
            if line_type < 0.01: # Rare occurrence of key terms
                line = f"Special fact about {random.choice(words)}: " + " ".join(random.choices(random_words, k=10)) + "\n"
            else:
                line = " ".join(random.choices(random_words, k=15)) + "\n"
            f.write(line)
            current_size += len(line)

if __name__ == "__main__":
    os.makedirs("data_heavy", exist_ok=True)
    print("Generating 100MB of test data...")
    generate_garbage(100, "data_heavy/big_data.txt")
    print("Done.")
