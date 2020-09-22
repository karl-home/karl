mkdir -p $HOME/karl/logs
cat $HOME/karl/hosts.txt | while read machine
do
    echo "sync $USER@${machine}"
    rsync -rtuv $HOME/karl $USER@${machine}:$HOME/
done
