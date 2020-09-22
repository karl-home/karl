if [ "$#" -le 0 ]
then
    echo "Usage: ./start.sh {NUM_SERVICES}"
else
    cat $HOME/karl/hosts.txt | while read machine
    do
        echo "starting $1 service(s) on ${machine}"
        for i in $(seq 1 1 $1)
        do
	    ssh $USER@${machine} \
		    "RUST_LOG=karl=debug,service=debug \
		    $HOME/karl/target/release/service > \
		    $HOME/karl/logs/service$i.txt 2>&1 \
		    &" &
        done
    done
fi
