#!/system/bin/sh
cd /data/local/tmp/firnbench
for n in $(ls | grep '\.firn$' | sed 's/\.firn$//'); do
  for r in 1 2 3 4 5; do
    for k in firn c; do
      echo "$n $k $(./runner ./$n.$k)"
    done
  done
  sleep 3
done
echo BENCH_DONE
