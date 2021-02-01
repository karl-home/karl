#!/bin/bash
echo "Node binary..."
nvm install 12.19.0
mkdir bin
cp $(which node) bin/node

echo "Node dependencies..."
npm install deepspeech
rm package-lock.json

echo "Downloading models..."
curl -L https://github.com/mozilla/DeepSpeech/releases/download/v0.8.1/deepspeech-0.8.1-models.pbmm -o models.pbmm
curl -L https://github.com/mozilla/DeepSpeech/releases/download/v0.8.1/deepspeech-0.8.1-models.scorer -o models.scorer
