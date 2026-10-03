#include <stdio.h>
static long long kern(const long long*z, const long long*w, int n){
    long long summe=0;
    for(int r=0;r<n;r++){
        long long a0=0,a1=0,a2=0,a3=0;
        for(int k=0;k<8;k++){
            long long w0=w[k*2], w1=w[k*2+1];
            const long long*p=z+k*64+(r%64);
            long long z0=p[0],z1=p[1],z2=p[2],z3=p[3];
            a0 += z0*w0 - z1*w1;
            a1 += z1*w1 + z0*w0;
            a2 += z2*w0 - z3*w1;
            a3 += z3*w1 + z2*w0;
        }
        summe += a0+a1+a2+a3;
    }
    return summe;
}
int main(void){
    static long long z[4096], w[16];
    for(int i=0;i<4096;i++) z[i]=i%97;
    for(int i=0;i<16;i++) w[i]=i%7;
    printf("%lld\n",kern(z,w,2000000));
    return 0;
}
