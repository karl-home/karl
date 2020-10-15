STT_PATH="$HOME/.karl/local/stt"
mkdir -p $STT_PATH
echo "STT_PATH is at \"$STT_PATH\""

# Setup python dependencies
echo "Using Python installation at $(which python3.6)..."
cp $(which python3.6) $STT_PATH/bin/python
virtualenv -p python3.6 sttenv
echo "Installing Python depenedencies..."
source sttenv/bin/activate
pip install deepspeech
cp -r sttenv/lib $STT_PATH/lib
deactivate
rm -rf sttenv

# Move client.py
cp data/stt/client.py $STT_PATH/client.py

# Download models
echo "Downloading models..."
curl -L https://github.com/mozilla/DeepSpeech/releases/download/v0.8.1/deepspeech-0.8.1-models.pbmm -o $STT_PATH/models.pbmm
curl -L https://github.com/mozilla/DeepSpeech/releases/download/v0.8.1/deepspeech-0.8.1-models.scorer -o $STT_PATH/models.scorer