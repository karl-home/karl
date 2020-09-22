cat $HOME/karl/hosts.txt | while read machine
do
    echo "init $USER@${machine}"
    ssh $USER@${machine} \
	    "rm -rf $HOME/karl/logs/* && \
	    $HOME/karl/scripts/init.sh > $HOME/karl/logs/init.txt 2>&1 &" &
done
