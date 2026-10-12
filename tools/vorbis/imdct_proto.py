import numpy as np
def direct(X):
    M=len(X); N=2*M
    n=np.arange(N)[:,None]; k=np.arange(M)[None,:]
    return (np.cos(2*np.pi/N*(n+0.5+N/4)*(k+0.5))*X[None,:]).sum(1)
def dct4(X):
    M=len(X); h=M//2
    j=np.arange(h)
    t=(X[2*j]+1j*X[M-1-2*j])*np.exp(-1j*np.pi*(4*j+1)/(4*M))
    T=np.fft.fft(t)
    u=T*np.exp(-1j*np.pi*j/M)
    c=np.zeros(M)
    c[2*j]=u.real
    c[M-1-2*j]=-u.imag
    return c
def imdct(X):
    M=len(X); c=dct4(X); y=np.zeros(2*M)
    h=M//2
    n=np.arange(h); y[n]=c[n+h]
    n=np.arange(h,3*h); y[n]=-c[3*h-1-n]
    n=np.arange(3*h,4*h); y[n]=-c[n-3*h]
    return y
X=np.random.randn(64)
print(np.abs(direct(X)-imdct(X)).max())
def dct4_direct(X):
    M=len(X); n=np.arange(M)[:,None]; k=np.arange(M)[None,:]
    return (np.cos(np.pi/M*(n+0.5)*(k+0.5))*X[None,:]).sum(1)
c=dct4_direct(X); print(np.abs(c-dct4(X)).max())
# try variants
M=len(X); h=M//2; j=np.arange(h)
for pre in [1,2]:
  for post in [1,2]:
    t=(X[2*j]+1j*X[M-1-2*j])*np.exp(-1j*np.pi*(4*j+1)/(4*M))
    T=np.fft.fft(t)
    # derive: c[2j] + i*(-c[M-1-2j])?
    u=T*np.exp(-1j*np.pi*j/M)
    print(np.abs(u.real-c[2*j]).max(), np.abs(-u.imag-c[M-1-2*j]).max(), np.abs(u.imag-c[M-1-2*j]).max())
    break
  break
