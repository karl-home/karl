if [ $# -eq 0 ]; then
	echo "Error: no arguments supplied"
	echo "1=none,2=pubsub,3=cold,4=warm"
	exit
fi

cargo b --release
if [ $1 -eq "1" ]; then
	sudo RUST_LOG=h2=info,hyper=info,tower=info,debug ./target/release/karl-host --port 59583 --pubsub 0 --cold-cache 0 --warm-cache 0
elif [ $1 -eq "2" ]; then
	sudo RUST_LOG=h2=info,hyper=info,tower=info,debug ./target/release/karl-host --port 59583 --pubsub 1 --cold-cache 0 --warm-cache 0
elif [ $1 -eq "3" ]; then
	sudo RUST_LOG=h2=info,hyper=info,tower=info,debug ./target/release/karl-host --port 59583 --pubsub 1 --cold-cache 1 --warm-cache 0
elif [ $1 -eq "4" ]; then
	sudo RUST_LOG=h2=info,hyper=info,tower=info,debug ./target/release/karl-host --port 59583 --pubsub 1 --cold-cache 1 --warm-cache 1
else
	echo "1=none,2=pubsub,3=cold,4=warm"
fi