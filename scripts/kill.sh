cat $HOME/karl/hosts.txt | while read machine
do
    echo "killing karl services on machine $USER@${machine}"
    ssh $USER@${machine} "ps -ef | grep 'karl/target' | grep -v grep | awk '{print \$2}'" &
    ssh $USER@${machine} \
        "ps -ef | grep 'karl/target' | grep -v grep | awk '{print \$2}' | xargs -r kill -9" &
done
