#
# Copyright 2020 Picovoice Inc.
#
# You may not use this file except in compliance with the license. A copy of the license is located in the "LICENSE"
# file accompanying this source.
#
# Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on
# an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the License for the
# specific language governing permissions and limitations under the License.
#

import argparse

import os
import json
import random
import karl
import soundfile
from picovoice import Picovoice

def read_file(api):
    audio_path = 'tmp.wav'
    with open(audio_path, 'wb') as f:
        audio_bytes = api.get_triggered()
        f.write(audio_bytes)
    return audio_path

def main():
    api = karl.KarlSDK()

    def wake_word_callback():
        print('[wake word]\n')

    def inference_callback(inference):
        print(inference)
        if inference.is_understood:
            # data = json.dumps(inference.slots, indent=2).encode('utf-8')
            if inference.intent == 'orderBeverage':
                tag = 'search'
                intent = { 'query': 'what is the meaning of life?' }
                data = json.dumps(intent, indent=2).encode('utf-8')
                api.push(tag, data)
            #    pass
            # elif inference.intent in ['changeLightState', 'changeLightStateOff']:
            #     api.push('light', data)
            # elif inference.intent == 'googleSearch':
            #     api.push('search', data)
            else:
                print('Unknown intent: {}'.format(inference.intent))
        else:
            print("Didn't understand the command.\n")

    pv = Picovoice(
        keyword_path='picovoice_linux.ppn',
        wake_word_callback=wake_word_callback,
        context_path='coffee_maker_linux.rhn',
        inference_callback=inference_callback,
        porcupine_sensitivity=0.5,
        rhino_sensitivity=0.5)

    file = read_file(api)
    # file = 'picovoice-coffee.wav'
    audio, sample_rate = soundfile.read(file, dtype='int16')
    if audio.ndim == 2:
        print("Picovoice processes single-channel audio but stereo file is provided. Processing left channel only.")
        audio = audio[0, :]

    if sample_rate != pv.sample_rate:
        raise ValueError("Input audio file should have a sample rate of %d. got %d" % (pv.sample_rate, sample_rate))

    for i in range(len(audio) // pv.frame_length):
        frame = audio[i * pv.frame_length:(i + 1) * pv.frame_length]
        pv.process(frame)

    pv.delete()


if __name__ == '__main__':
    main()
