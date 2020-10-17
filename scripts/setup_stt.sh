STT_PATH="$HOME/.karl/local/stt"
PYTHON=python3.6
mkdir -p $STT_PATH
mkdir -p $STT_PATH/bin
echo "STT_PATH is at \"$STT_PATH\""

# Copy client.py
echo "Copying client.py..."
cp data/stt/client.py $STT_PATH/client.py
# Copy python
echo "Using Python installation at $(which $PYTHON)..."
cp $(which $PYTHON) $STT_PATH/bin/python

# Setup python dependencies
rm -rf $STT_PATH/lib
virtualenv -p $PYTHON sttenv
echo "Installing Python depenedencies..."
source sttenv/bin/activate
pip install deepspeech
cp -r sttenv/lib $STT_PATH/lib
deactivate
rm -rf sttenv

# Download models
echo "Downloading models..."
curl -L https://github.com/mozilla/DeepSpeech/releases/download/v0.8.1/deepspeech-0.8.1-models.pbmm -o $STT_PATH/models.pbmm
curl -L https://github.com/mozilla/DeepSpeech/releases/download/v0.8.1/deepspeech-0.8.1-models.scorer -o $STT_PATH/models.scorer