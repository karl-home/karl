LOGS="$HOME/karl/scripts/logs"
rm -rf $LOGS
cat $HOME/karl/hosts.txt | while read machine
do
    mkdir -p $LOGS/${machine}
    echo "copy logs from $USER@${machine}"
    scp -r $USER@${machine}:$HOME/karl/logs/* $LOGS/${machine}/ &
done
