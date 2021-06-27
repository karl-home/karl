if [ $# -eq 0 ]; then
	echo "Error: no arguments supplied"
	echo "1=none,2=pubsub,3=cold,4=warm"
	exit
fi

cargo b --release
if [ $1 -eq "1" ]; then
	RUST_LOG=info,karl=debug ./target/release/karl-controller --autoconfirm --pubsub-enabled 0 --caching-enabled 0
elif [ $1 -eq "2" ]; then
	RUST_LOG=info,karl=debug ./target/release/karl-controller --autoconfirm --pubsub-enabled 1 --caching-enabled 0
elif [ $1 -eq "3" ]; then
	RUST_LOG=info,karl=debug ./target/release/karl-controller --autoconfirm --pubsub-enabled 1 --caching-enabled 1
elif [ $1 -eq "4" ]; then
	RUST_LOG=info,karl=debug ./target/release/karl-controller --autoconfirm --pubsub-enabled 1 --caching-enabled 1
else
	echo "1=none,2=pubsub,3=cold,4=warm"
fi
