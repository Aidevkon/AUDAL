import numpy as np
from libnmfd.core.nmfconv import nmfd

def generate_iter1():
    A = np.fromfile("V.bin", dtype='<f8').reshape(513, 130)
    tensor_W = np.fromfile("init_W_nmfd.bin", dtype='<f8').reshape(513, 4, 8)
    init_W_nmfd = []
    for r in range(4):
        init_W_nmfd.append(tensor_W[:, r, :])
    init_H = np.fromfile("init_H.bin", dtype='<f8').reshape(4, 130)
    
    W_out, H_out, nmfd_V_out, cost_func_out, tensor_W_out = nmfd(
        A, num_comp=4, num_template_frames=8,
        num_iter=1, init_W=init_W_nmfd, init_H=init_H.copy()
    )
    
    np.ascontiguousarray(tensor_W_out).astype('<f8', copy=False).tofile("iter1_W.bin")
    np.ascontiguousarray(H_out).astype('<f8', copy=False).tofile("iter1_H.bin")
    print("Exported iter1")

if __name__ == "__main__":
    generate_iter1()
