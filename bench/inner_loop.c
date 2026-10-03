#include <stdio.h>
static float kern(const float*z, const float*w, int n){
    float summe=0.0f;
    for(int r=0;r<n;r++){
        float a0=0,a1=0,a2=0,a3=0;
        for(int k=0;k<8;k++){
            float w0=w[k*2], w1=w[k*2+1];
            const float*p=z+k*64+(r%64);
            float z0=p[0],z1=p[1],z2=p[2],z3=p[3];
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
    static float z[4096], w[16];
    for(int i=0;i<4096;i++) z[i]=(i%97)*0.01f;
    for(int i=0;i<16;i++) w[i]=(i%7)*0.125f;
    printf("%lld\n",(long long)kern(z,w,2000000));
    return 0;
}
