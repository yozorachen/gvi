N=50
SIZE=1K

for i in $(seq 1 $N); do
        FILENAME=$(printf "file_%02d.bin" $i)
        truncate -s $SIZE $FILENAME
        echo "Generated $FILENAME"
done
