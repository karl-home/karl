copy_to_storage() {
	export LOCAL="$HOME/.karl/storage"
	mkdir -p $LOCAL/wyzecam $LOCAL/almond

	cp -r data/person-detection/* $LOCAL/wyzecam
	rm -r $LOCAL/wyzecam/PennFudanPed
	rm $LOCAL/wyzecam/setup.sh

	cp -r data/stt_node/* $LOCAL/almond
	rm $LOCAL/almond/weather.wav
	rm $LOCAL/almond/setup.sh
}

cd data/person-detection
source setup.sh
cd ../stt_node
source setup.sh
cd ../hello_world
source setup.sh
cd ../../cloud
source setup.sh
cd ..
copy_to_storage
