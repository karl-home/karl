STT_PATH="$HOME/.karl/local/stt_node"
PYTHON=python3.6

sudo apt-get install -y npm
curl -o- https://raw.githubusercontent.com/creationix/nvm/v0.33.0/install.sh | bash
source ~/.bashrc
nvm install 12.19.0
rm -rf node_modules $STT_PATH/node_modules
npm install deepspeech

mkdir -p $STT_PATH
mkdir -p $STT_PATH/bin
echo "STT_PATH is at \"$STT_PATH\""

echo "Copying main.js..."
cp data/stt_node/main.js $STT_PATH/main.js
echo "Using Node (12.19.0) installation at $(which node)..."
cp $(which node) $STT_PATH/bin/node

# Setup dependencies
mv node_modules/ $STT_PATH/node_modules/
rm package-lock.json

# Download models
echo "Downloading models..."
curl -L https://github.com/mozilla/DeepSpeech/releases/download/v0.8.1/deepspeech-0.8.1-models.pbmm -o $STT_PATH/models.pbmm
curl -L https://github.com/mozilla/DeepSpeech/releases/download/v0.8.1/deepspeech-0.8.1-models.scorer -o $STT_PATH/models.scorer
