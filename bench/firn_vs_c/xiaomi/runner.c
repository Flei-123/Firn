#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/wait.h>
#include <sys/resource.h>
#include <fcntl.h>
#include <string.h>
int main(int argc,char**argv){
  /* usage: runner bin  -> prints "cpu_seconds checksum" */
  int pf[2]; pipe(pf);
  pid_t p=fork();
  if(!p){dup2(pf[1],1);close(pf[0]);int n=open("/dev/null",O_WRONLY);dup2(n,2);execv(argv[1],argv+1);_exit(127);}
  close(pf[1]);
  char buf[512];int len=0,r;
  while((r=read(pf[0],buf+len,sizeof(buf)-1-len))>0)len+=r;
  buf[len]=0; for(char*q=buf;*q;q++) if(*q=='\n'){*q=0;break;}
  int st;struct rusage ru;wait4(p,&st,0,&ru);
  double t=ru.ru_utime.tv_sec+ru.ru_utime.tv_usec/1e6+ru.ru_stime.tv_sec+ru.ru_stime.tv_usec/1e6;
  if(!WIFEXITED(st)||WEXITSTATUS(st)){printf("ERR %d\n",st);return 1;}
  printf("%.4f %s\n",t,buf);return 0;}
