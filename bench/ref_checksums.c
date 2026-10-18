#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
/* Reference for bench/EXPECTED_REF.txt; build: gcc -O1 -ffp-contract=off bench/ref_checksums.c -o ref */
#define MOD 1000000007ULL
static int cmp(const void*a,const void*b){int x=*(const int*)a,y=*(const int*)b;return (x>y)-(x<y);}
int main(void){
  /* primes */
  enum{L=5000000}; static unsigned char c[L]; uint64_t cnt=0, psum=0;
  for(long i=2;i<L;i++){ if(!c[i]){cnt++; psum+=(uint64_t)i; for(long j=i*i;j<L;j+=i)c[j]=1;} }
  printf("primes=%llu\n",(unsigned long long)((cnt*1000003ULL+psum)%MOD));
  /* sort */
  int n=1000000; int*v=malloc(n*sizeof(int)); uint64_t s=12345;
  for(int i=0;i<n;i++){ s=(s*1103515245ULL+12345ULL)&0x7fffffffULL; v[i]=(int)s; }
  qsort(v,n,sizeof(int),cmp);
  uint64_t t=0; for(int i=0;i<n;i++) t+=(uint64_t)v[i]*(uint64_t)((i%251)+1);
  printf("sort=%llu\n",(unsigned long long)(t%MOD));
  /* mandelbrot */
  long sum=0; double dx=3.5/800.0, dy=2.0/600.0;
  for(int py=0;py<600;py++){ double ci=-1.0+py*dy;
    for(int px=0;px<800;px++){ double cr=-2.5+px*dx; double zr=0,zi=0; int k=0;
      while(k<200){ double zr2=zr*zr, zi2=zi*zi; if(zr2+zi2>4.0)break; zi=(2.0*zr)*zi+ci; zr=(zr2-zi2)+cr; k++; }
      sum+=k; } }
  printf("mandel=%ld\n",sum);
  free(v);
  return 0; }
