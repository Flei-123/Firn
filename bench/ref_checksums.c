#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
static int cmp(const void*a,const void*b){int x=*(const int*)a,y=*(const int*)b;return (x>y)-(x<y);}
int main(void){
  /* primes */
  enum{L=5000000}; static unsigned char c[L]; long cnt=0;
  for(long i=2;i<L;i++){ if(!c[i]){cnt++; for(long j=i*i;j<L;j+=i)c[j]=1;} }
  printf("primes=%ld\n",cnt);
  /* sort */
  int n=1000000; int*v=malloc(n*sizeof(int)); uint64_t s=12345;
  for(int i=0;i<n;i++){ s=(s*1103515245ULL+12345ULL)&0x7fffffffULL; v[i]=(int)s; }
  qsort(v,n,sizeof(int),cmp);
  uint64_t h=0; for(int i=0;i<n;i++) h=(h*31ULL+(uint64_t)(int64_t)v[i])&0xffffffffULL;
  printf("sort=%llu\n",(unsigned long long)h);
  /* mandelbrot */
  long sum=0;
  for(int py=0;py<600;py++){ double ci=-1.2+2.4*py/600;
    for(int px=0;px<800;px++){ double cr=-2.0+3.0*px/800; double zr=0,zi=0; int k=0;
      while(k<200){ double zr2=zr*zr, zi2=zi*zi; if(zr2+zi2>4.0)break; zi=2.0*zr*zi+ci; zr=zr2-zi2+cr; k++; }
      sum+=k; } }
  printf("mandel=%ld\n",sum);
  return 0; }
