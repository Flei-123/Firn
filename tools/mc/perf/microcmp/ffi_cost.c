#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <time.h>
static uint64_t now_ns(void){struct timespec t;clock_gettime(CLOCK_MONOTONIC,&t);return t.tv_sec*1000000000ull+t.tv_nsec;}
int main(void){
    long (*volatile f)(long)=labs;
    uint64_t best=~0ull,sum=0;const size_t reps=20000000;
    for(int r=0;r<5;r++){uint64_t t0=now_ns();for(size_t i=0;i<reps;i++)sum+=f((long)i);uint64_t dt=now_ns()-t0;if(dt<best)best=dt;}
    printf("RESULT lang=c call_ns_x100=%llu sum=%llu\n",(unsigned long long)(best*100/reps),(unsigned long long)sum);
    return 0;}
