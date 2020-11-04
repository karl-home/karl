#!/bin/bash
DATA_PATH="$(pwd)/data/stt_node"
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
check_original_file "$DATA_PATH/main.js"
check_original_file "$DATA_PATH/weather.wav"

# Binary
NVM=$(which nvm)
if [ -z "$NVM" ]; then
	echo "No nvm installation. Trying to install."
	if [[ "$OSTYPE" == "linux-gnu"* ]]; then
		sudo apt-get install -y npm
		curl -o- https://raw.githubusercontent.com/creationix/nvm/v0.33.0/install.sh | bash
		source ~/.bashrc
		NVM=$(which nvm)
		if [ -z "$NVM" ]; then
			echo "Failed to install nvm on $OSTYPE"
			exit
		fi
	elif [[ "$OSTYPE" == "darwin"* ]]; then
		brew update && brew install nvm
		export NVM_DIR="$HOME/.nvm"
		[ -s "/usr/local/opt/nvm/nvm.sh" ] && . "/usr/local/opt/nvm/nvm.sh"  # This loads nvm
		[ -s "/usr/local/opt/nvm/etc/bash_completion.d/nvm" ] && . "/usr/local/opt/nvm/etc/bash_completion.d/nvm"
	else
		echo "Can't install nvm on $OSTYPE"
		exit
	fi
fi
nvm install 12.19.0
NODE=$(which node)
[ -z "$NODE" ]
mkdir -p $DATA_PATH/bin
echo "Using node installation at $NODE"
cp $NODE $DATA_PATH/bin/node

# Node dependencies
NPM=$(which npm)
if [ -z "$NPM" ]; then
	echo "No npm installation"
	exit
else
	echo "Installing deepspeech dependency"
	npm install deepspeech
	rm -rf $DATA_PATH/node_modules
	mv node_modules $DATA_PATH/node_modules
	rm package-lock.json
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
STT_NODE_PATH="$HOME/.karl/local/stt_node"
rm -rf $STT_NODE_PATH
cp -r $DATA_PATH $STT_NODE_PATH
rm -rf $STT_NODE_PATH/weather.wav
echo "STT karl cache written to $STT_NODE_PATH"
