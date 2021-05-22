# Setup Python virtual environment
virtualenv -p python3 env
source env/bin/activate
pip install -r requirements.txt

# Generate the gRPC wrappers
python -m grpc_tools.protoc -I../../protos \
    --python_out=. --grpc_python_out=. ../../protos/request.proto

# Download the PennFudan dataset
wget https://www.cis.upenn.edu/~jshi/ped_html/PennFudanPed.zip
unzip PennFudanPed.zip
rm PennFudanPed.zip

# Download the maskrcnn model
mkdir -p torch/checkpoints
wget https://download.pytorch.org/models/maskrcnn_resnet50_fpn_coco-bf2d0c1e.pth
mv maskrcnn_resnet50_fpn_coco-bf2d0c1e.pth torch/checkpoints/

# Deactivate
deactivate

run_firejail() {
    firejail --private=. \
    --netfilter=/etc/firejail/karl.net \
    --env=GLOBAL_MODULE_ID=person_detection \
    --env=MODULE_ID=person_detection-2917637381 \
    --env=TRIGGERED_TAG=image \
    --env=PROCESS_TOKEN=7mb7zUL4s9Gs2F2WTe6v0qrFRemXuOgy \
    --env=TRIGGERED_TIMESTAMP=2021-04-26T14:49:04.499119935-07:00 \
    ./env/bin/python detect.py
}
