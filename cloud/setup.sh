# Setup Python virtual environment
virtualenv -p python3 env
source env/bin/activate
pip install --upgrade google-cloud-speech
deactivate
