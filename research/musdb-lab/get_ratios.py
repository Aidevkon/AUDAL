import numpy as np

# Let's load the data from factory_v5_discriminative.py but just the matrices
import sys
sys.path.append("/home/aidevcon/Documents/creator-os/research/musdb-lab")
import factory_v5_discriminative as v5

V_voice = np.load("/home/aidevcon/Documents/creator-os/research/musdb-lab/V_voice.npy") if False else None

