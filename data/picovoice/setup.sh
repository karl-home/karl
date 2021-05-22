# Setup Python virtual environment
virtualenv -p python3 env
source env/bin/activate
pip install -r requirements.txt

# Generate the gRPC wrappers
python -m grpc_tools.protoc -I../../protos \
    --python_out=. --grpc_python_out=. ../../protos/request.proto

# Update path to sndfile library
sed -i "s/_find_library('sndfile')/'libsndfile.so.1'/g" \
    env/lib/python3.8/site-packages/soundfile.py

# Deactivate
deactivate

run_firejail() {
    firejail --private=. \
    --netfilter=/etc/firejail/karl.net \
    --env=GLOBAL_MODULE_ID=command_classifier \
    --env=MODULE_ID=command_classifier-2917637381 \
    --env=TRIGGERED_TAG=sound \
    --env=PROCESS_TOKEN=7mb7zUL4s9Gs2F2WTe6v0qrFRemXuOgy \
    --env=TRIGGERED_TIMESTAMP=2021-04-26T14:49:04.499119935-07:00 \
    ./env/bin/python picovoice_demo_file.py
}