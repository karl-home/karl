#!/bin/bash
DATA_PATH="$(pwd)/data/stt"
if [ ! -d "$DATA_PATH" ]; then
	echo "Data path at $DATA_PATH does not exist"
	exit
fi

# Check original files exist
check_original_file() {
	if [ -z "$1" ]; then echo "No argument supplied"; exit; fi
	if [ ! -f "$1" ]; then
		echo "File does not exist at $1. Checkout the original branch?"
		exit
	fi
}
check_original_file "$DATA_PATH/client.py"
check_original_file "$DATA_PATH/audio/2830-3980-0043.wav"

# Binary
PYTHON=$(which python3.6)
if [ -z "$PYTHON" ]; then
 	echo "No python3.6 installation"
 	exit
else
	mkdir -p $DATA_PATH/bin
	echo "Using python3.6 installation at $PYTHON"
	cp $PYTHON $DATA_PATH/bin/python
fi

# Python dependencies
VIRTUALENV=$(which virtualenv)
if [ -z "$VIRTUALENV" ]; then
	echo "No virtualenv installation"
	exit
else
	echo "Installing deepspeech dependency"
	virtualenv -p $PYTHON tmp-env
	source tmp-env/bin/activate
	pip install deepspeech
	rm -rf $DATA_PATH/lib
	mv tmp-env/lib $DATA_PATH/lib
	deactivate
	rm -rf tmp-env
fi

# Download models
echo "Downloading models..."
if [ ! -f $DATA_PATH/models.pbmm ]; then
	curl -L https://github.com/mozilla/DeepSpeech/releases/download/v0.8.1/deepspeech-0.8.1-models.pbmm -o $DATA_PATH/models.pbmm
else
	echo "$DATA_PATH/models.pbmm exists"
fi
if [ ! -f $DATA_PATH/models.scorer ]; then
	curl -L https://github.com/mozilla/DeepSpeech/releases/download/v0.8.1/deepspeech-0.8.1-models.scorer -o $DATA_PATH/models.scorer
else
	echo "$DATA_PATH/models.scorer exists"
fi

# Copy data path to karl cache
STT_PATH="$HOME/.karl/local/stt"
rm -rf $STT_PATH
cp -r $DATA_PATH $STT_PATH
rm -rf $STT_PATH/audio
echo "STT karl cache written to $STT_PATH"
